use harfrust::{FontRef, ShapeOptions, ShaperData, UnicodeBuffer};
use rustc_hash::FxHashMap;

pub const ATLAS_SIZE: usize = 1024;
const ATLAS_PADDING: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontTag {
    Main,
    Icon,
}

/// Identifies one concrete font face within a [`FontCache`]. For Phase 0 there
/// is a single primary face per cache (id 0); Phase 1 (fallback) and bold/italic
/// selection add more faces and resolve a logical request to a `FaceId` *before*
/// shaping. Glyph ids are face-relative, so the face is part of the raster key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FaceId(pub u16);

const PRIMARY_FACE: FaceId = FaceId(0);

/// U+FE0F: requests emoji (color) presentation of the preceding base codepoint.
const EMOJI_VARIATION_SELECTOR: char = '\u{FE0F}';

/// Synthetic styling that changes a glyph's *bitmap* for the same face+glyph
/// (e.g. faux bold/oblique). Folded into the raster cache key. Always default in
/// Phase 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RasterFlags(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasRegion {
    pub atlas_index: usize,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl AtlasRegion {
    pub fn uv(self, atlas_width: usize, atlas_height: usize) -> (f32, f32, f32, f32) {
        (
            self.x as f32 / atlas_width as f32,
            self.y as f32 / atlas_height as f32,
            (self.x + self.width) as f32 / atlas_width as f32,
            (self.y + self.height) as f32 / atlas_height as f32,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TextPiece {
    pub atlas_index: usize,
    pub subrect_px: AtlasRegion,
    pub advance: f32,
    /// Byte offset of this glyph's grapheme cluster in the *source* string. With
    /// shaping the glyph<->byte mapping is not 1:1 (ligatures, reordering), so this
    /// is what caret/hit-testing keys off of (used in later phases).
    pub cluster: usize,
    pub offset_px: (f32, f32),
    /// True for a color (RGBA) glyph: `atlas_index` indexes the color atlases and
    /// the renderer samples it directly instead of tinting an alpha mask.
    pub color: bool,
}

#[derive(Clone, Debug, Default)]
pub struct TextRun {
    pub pieces: Vec<TextPiece>,
    pub dim: (f32, f32),
}

#[derive(Clone, Debug)]
pub struct AtlasUpload {
    pub atlas_index: usize,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
    /// True if this upload targets a color (RGBA) atlas rather than an alpha atlas.
    pub color: bool,
}

#[derive(Clone, Debug)]
pub struct Atlas {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
    /// 1 for an alpha-coverage atlas (R8), 4 for a color atlas (RGBA8).
    bytes_per_pixel: usize,
    next_x: usize,
    next_y: usize,
    row_height: usize,
    texture_id: u32,
}

impl Atlas {
    pub fn new() -> Self {
        Self::with_bpp(1)
    }

    pub fn new_color() -> Self {
        Self::with_bpp(4)
    }

    fn with_bpp(bytes_per_pixel: usize) -> Self {
        Self {
            data: vec![0; ATLAS_SIZE * ATLAS_SIZE * bytes_per_pixel],
            width: ATLAS_SIZE,
            height: ATLAS_SIZE,
            bytes_per_pixel,
            next_x: ATLAS_PADDING,
            next_y: ATLAS_PADDING,
            row_height: 0,
            texture_id: 0,
        }
    }

    pub fn bytes_per_pixel(&self) -> usize {
        self.bytes_per_pixel
    }

    pub fn texture_id(&self) -> u32 {
        self.texture_id
    }

    pub fn set_texture_id(&mut self, texture_id: u32) {
        self.texture_id = texture_id;
    }

    pub fn allocate(&mut self, width: usize, height: usize) -> Option<(usize, usize)> {
        let alloc_w = width + ATLAS_PADDING * 2;
        let alloc_h = height + ATLAS_PADDING * 2;
        if alloc_w > self.width || alloc_h > self.height {
            return None;
        }

        if self.next_x + alloc_w > self.width {
            self.next_x = ATLAS_PADDING;
            self.next_y += self.row_height;
            self.row_height = 0;
        }

        if self.next_y + alloc_h > self.height {
            return None;
        }

        let x = self.next_x + ATLAS_PADDING;
        let y = self.next_y + ATLAS_PADDING;
        self.next_x += alloc_w;
        self.row_height = self.row_height.max(alloc_h);
        Some((x, y))
    }

    fn write_region(&mut self, x: usize, y: usize, width: usize, height: usize, data: &[u8]) {
        let bpp = self.bytes_per_pixel;
        debug_assert_eq!(data.len(), width * height * bpp);
        let row_bytes = width * bpp;
        for row in 0..height {
            let dst = ((y + row) * self.width + x) * bpp;
            let src = row * row_bytes;
            self.data[dst..dst + row_bytes].copy_from_slice(&data[src..src + row_bytes]);
        }
    }
}

/// A rasterized glyph bitmap plus its placement metrics. The advance is *not*
/// here: it comes from the shaper, not the rasterizer. `color` glyphs carry RGBA
/// (4 bytes/px, premultiplied); otherwise the bitmap is 8-bit alpha coverage.
pub struct RasterizedGlyph {
    pub width: usize,
    pub height: usize,
    pub xoff: f32,
    pub yoff: f32,
    pub color: bool,
    pub bitmap: Vec<u8>,
}

impl RasterizedGlyph {
    fn empty() -> Self {
        Self {
            width: 0,
            height: 0,
            xoff: 0.0,
            yoff: 0.0,
            color: false,
            bitmap: Vec::new(),
        }
    }
}

/// Collects a skrifa glyph outline into zeno path commands. skrifa emits pixel
/// coordinates with +Y up; zeno (and our atlas) are +Y down, so we negate Y. The
/// resulting `Placement` then yields top-left offsets directly in screen space.
#[derive(Default)]
struct ZenoPen {
    commands: Vec<zeno::Command>,
}

impl skrifa::outline::OutlinePen for ZenoPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands
            .push(zeno::Command::MoveTo(zeno::Vector::new(x, -y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.commands
            .push(zeno::Command::LineTo(zeno::Vector::new(x, -y)));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.commands.push(zeno::Command::QuadTo(
            zeno::Vector::new(cx0, -cy0),
            zeno::Vector::new(x, -y),
        ));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.commands.push(zeno::Command::CurveTo(
            zeno::Vector::new(cx0, -cy0),
            zeno::Vector::new(cx1, -cy1),
            zeno::Vector::new(x, -y),
        ));
    }
    fn close(&mut self) {
        self.commands.push(zeno::Command::Close);
    }
}

/// Rasterize one glyph outline to an 8-bit coverage mask via skrifa + zeno.
fn rasterize_outline(font: &skrifa::FontRef, glyph_id: u16, px: f32) -> Option<RasterizedGlyph> {
    use skrifa::MetadataProvider;

    if px <= 0.0 {
        return None;
    }
    let outlines = font.outline_glyphs();
    let glyph = outlines.get(skrifa::GlyphId::new(glyph_id as u32))?;

    let mut pen = ZenoPen::default();
    let settings = skrifa::outline::DrawSettings::unhinted(
        skrifa::instance::Size::new(px),
        skrifa::instance::LocationRef::default(),
    );
    glyph.draw(settings, &mut pen).ok()?;
    if pen.commands.is_empty() {
        // Whitespace and other empty glyphs: valid, just no coverage.
        return Some(RasterizedGlyph::empty());
    }

    let (bitmap, placement) = zeno::Mask::new(pen.commands.as_slice()).render();
    Some(RasterizedGlyph {
        width: placement.width as usize,
        height: placement.height as usize,
        xoff: placement.left as f32,
        yoff: placement.top as f32,
        color: false,
        bitmap,
    })
}

/// Rasterize a color bitmap glyph (sbix/CBDT, e.g. Apple Color Emoji) to RGBA at
/// the target pixel size, or `None` if this glyph has no color bitmap. PNG strikes
/// are decoded and scaled; the result is positioned on the baseline like an outline.
fn rasterize_color_bitmap(
    font: &skrifa::FontRef,
    glyph_id: u16,
    px: f32,
) -> Option<RasterizedGlyph> {
    use skrifa::MetadataProvider;
    use skrifa::bitmap::{BitmapData, Origin};

    if px <= 0.0 {
        return None;
    }
    let strikes = font.bitmap_strikes();
    let bitmap = strikes.glyph_for_size(
        skrifa::instance::Size::new(px),
        skrifa::GlyphId::new(glyph_id as u32),
    )?;

    let (src_w, src_h, rgba) = match bitmap.data {
        BitmapData::Png(bytes) => decode_png_rgba(bytes)?,
        // Uncompressed strikes are rarer; skip for now (outline fallback handles it).
        _ => return None,
    };
    if src_w == 0 || src_h == 0 {
        return None;
    }

    // Scale the strike (native ppem) down to the requested pixel size.
    let scale = if bitmap.ppem_y > 0.0 {
        px / bitmap.ppem_y
    } else {
        1.0
    };
    let dst_w = ((src_w as f32 * scale).round() as usize).max(1);
    let dst_h = ((src_h as f32 * scale).round() as usize).max(1);
    let scaled = resize_rgba(&rgba, src_w, src_h, dst_w, dst_h);

    // Place on the baseline. inner_bearing is in pixels at the strike ppem.
    let xoff = bitmap.inner_bearing_x * scale;
    let oy = bitmap.inner_bearing_y * scale;
    let yoff = match bitmap.placement_origin {
        Origin::BottomLeft => -oy - dst_h as f32,
        Origin::TopLeft => -oy,
    };

    Some(RasterizedGlyph {
        width: dst_w,
        height: dst_h,
        xoff,
        yoff,
        color: true,
        bitmap: scaled,
    })
}

/// Decode a PNG byte stream to straight-alpha RGBA8. Returns (width, height, rgba).
fn decode_png_rgba(bytes: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    buf.truncate(info.buffer_size());

    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(w * h * 4);
            for px in buf.chunks_exact(3) {
                out.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            out
        }
        png::ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity(w * h * 4);
            for px in buf.chunks_exact(2) {
                out.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
            out
        }
        png::ColorType::Grayscale => {
            let mut out = Vec::with_capacity(w * h * 4);
            for &g in &buf {
                out.extend_from_slice(&[g, g, g, 255]);
            }
            out
        }
        png::ColorType::Indexed => return None,
    };
    Some((w, h, rgba))
}

/// Bilinear resize of an RGBA8 image. Adequate for downscaling emoji strikes.
fn resize_rgba(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    if sw == dw && sh == dh {
        return src.to_vec();
    }
    let mut out = vec![0u8; dw * dh * 4];
    for dy in 0..dh {
        let fy = (dy as f32 + 0.5) * sh as f32 / dh as f32 - 0.5;
        let y0 = fy.floor().max(0.0) as usize;
        let y1 = (y0 + 1).min(sh - 1);
        let wy = (fy - y0 as f32).clamp(0.0, 1.0);
        for dx in 0..dw {
            let fx = (dx as f32 + 0.5) * sw as f32 / dw as f32 - 0.5;
            let x0 = fx.floor().max(0.0) as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let wx = (fx - x0 as f32).clamp(0.0, 1.0);
            let di = (dy * dw + dx) * 4;
            for c in 0..4 {
                let p00 = src[(y0 * sw + x0) * 4 + c] as f32;
                let p10 = src[(y0 * sw + x1) * 4 + c] as f32;
                let p01 = src[(y1 * sw + x0) * 4 + c] as f32;
                let p11 = src[(y1 * sw + x1) * 4 + c] as f32;
                let top = p00 + (p10 - p00) * wx;
                let bot = p01 + (p11 - p01) * wx;
                out[di + c] = (top + (bot - top) * wy).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// One shaped glyph in source order: glyph id + position from the shaper, with
/// `cluster` being the byte offset into the source string.
#[derive(Clone, Copy)]
struct ShapedGlyph {
    glyph_id: u16,
    cluster: usize,
    advance: f32,
    x_offset: f32,
    y_offset: f32,
}

/// One item of a shaped run, independent of pen position so it can be cached
/// across frames. Tabs are layout-level (tab stops), not shaper output, so they
/// are kept as a distinct item. Each glyph carries the `FaceId` it came from
/// (fallback means a single run can span multiple faces).
#[derive(Clone, Copy)]
enum CachedItem {
    Tab { cluster: usize },
    Glyph { face: FaceId, glyph: ShapedGlyph },
}

/// A concrete font face: the owned bytes, the collection index, and the harfrust
/// shaping data built once. The cheap `FontRef`/`Shaper` (harfrust) and skrifa
/// `FontRef` are rebuilt per call; the expensive parse lives in `shaper_data`.
struct Face {
    bytes: Vec<u8>,
    index: u32,
    shaper_data: ShaperData,
    upem: f32,
}

impl Face {
    fn new(font_bytes: &[u8]) -> Self {
        Self::new_with_index(font_bytes, 0)
    }

    fn new_with_index(font_bytes: &[u8], index: u32) -> Self {
        let bytes = font_bytes.to_vec();
        let font = FontRef::from_index(&bytes, index).expect("harfrust failed to parse font");
        let shaper_data = ShaperData::new(&font);
        let upem = shaper_data.shaper(&font).build().units_per_em() as f32;
        Self {
            bytes,
            index,
            shaper_data,
            upem,
        }
    }

    /// A skrifa view of this face (for rasterization, coverage, and metrics).
    fn skrifa(&self) -> Option<skrifa::FontRef<'_>> {
        skrifa::FontRef::from_index(&self.bytes, self.index).ok()
    }

    fn covers(&self, c: char) -> bool {
        use skrifa::MetadataProvider;
        self.skrifa()
            .map(|font| font.charmap().map(c).is_some())
            .unwrap_or(false)
    }

    fn rasterize(&self, glyph_id: u16, px: f32) -> Option<RasterizedGlyph> {
        let font = self.skrifa()?;
        // Color bitmap glyphs (emoji) have no outline; try them first.
        if let Some(color) = rasterize_color_bitmap(&font, glyph_id, px) {
            return Some(color);
        }
        rasterize_outline(&font, glyph_id, px)
    }

    /// Shape one uniform-direction sub-run, appending glyphs in *visual* order to
    /// `out` (harfrust returns RTL runs already reversed). Clusters are relative to
    /// `text`; callers offset them to absolute positions. `rtl` forces the run's
    /// direction (set by BiDi), overriding script-based guessing.
    fn shape_into(&self, text: &str, px: f32, rtl: bool, out: &mut Vec<ShapedGlyph>) {
        let Ok(font) = FontRef::from_index(&self.bytes, self.index) else {
            return;
        };
        let shaper = self.shaper_data.shaper(&font).build();
        let scale = if self.upem > 0.0 { px / self.upem } else { 0.0 };

        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        // Guess script/language, then force the BiDi-resolved direction.
        buffer.guess_segment_properties();
        buffer.set_direction(if rtl {
            harfrust::Direction::RightToLeft
        } else {
            harfrust::Direction::LeftToRight
        });
        let glyphs = shaper.shape(buffer, ShapeOptions::default());

        let infos = glyphs.glyph_infos();
        let positions = glyphs.glyph_positions();
        out.reserve(infos.len());
        for (info, pos) in infos.iter().zip(positions) {
            out.push(ShapedGlyph {
                glyph_id: info.glyph_id as u16,
                cluster: info.cluster as usize,
                advance: pos.x_advance as f32 * scale,
                x_offset: pos.x_offset as f32 * scale,
                y_offset: pos.y_offset as f32 * scale,
            });
        }
    }

    fn line_height(&self, px: f32) -> f32 {
        use skrifa::MetadataProvider;
        self.skrifa()
            .map(|font| {
                let m = font.metrics(
                    skrifa::instance::Size::new(px),
                    skrifa::instance::LocationRef::default(),
                );
                m.ascent - m.descent + m.leading
            })
            .unwrap_or(px * 1.2)
    }
}

#[derive(Clone, Copy, Debug)]
struct RasterEntry {
    region: AtlasRegion,
    xoff: f32,
    yoff: f32,
    /// True if `region` indexes the color atlases (RGBA) rather than alpha atlases.
    color: bool,
}

/// Cap on the shaped-run cache before it is cleared wholesale. Bounds memory as
/// edits churn unique line strings; a real LRU is a future refinement.
const SHAPED_CACHE_CAP: usize = 8192;

pub struct FontCache {
    tag: FontTag,
    /// Face 0 is the primary; further entries are lazily-loaded fallback faces.
    faces: Vec<Face>,
    /// Resolution cache: which face renders a codepoint the primary lacks
    /// (`None` = no fallback found, render `.notdef`). Avoids per-frame OS calls.
    fallback_for: FxHashMap<char, Option<FaceId>>,
    /// Like `fallback_for` but for emoji-presentation requests (base + U+FE0F),
    /// resolving to a color emoji face even when the primary covers the text form.
    emoji_fallback_for: FxHashMap<char, Option<FaceId>>,
    /// Dedup loaded fallback faces by (path, collection index).
    face_by_key: FxHashMap<(String, u32), FaceId>,
    /// Shaped-run cache: hash(px, text) -> (text, items). Memoizes itemization +
    /// shaping (the expensive part) across frames; positioning + rasterization are
    /// cheap and run per call. Stores the source text to guard hash collisions.
    shaped_cache: FxHashMap<u64, (String, Vec<CachedItem>)>,
    /// Reusable buffer for the per-call positioning pass (avoids re-allocating).
    pos_scratch: Vec<CachedItem>,
    /// Glyph atlas cache keyed by a packed `u64`: face id, glyph id, quantized
    /// pixel size, raster flags. Hashed with FxHash on the hot path.
    raster_cache: FxHashMap<u64, RasterEntry>,
    atlases: Vec<Atlas>,
    /// Color (RGBA) atlases for emoji and other color glyphs, indexed separately.
    color_atlases: Vec<Atlas>,
    pending_uploads: Vec<AtlasUpload>,
    textures_dirty: bool,
}

impl FontCache {
    pub fn new(font_bytes: &[u8]) -> Self {
        Self::new_with_tag(FontTag::Main, font_bytes)
    }

    pub fn new_with_tag(tag: FontTag, font_bytes: &[u8]) -> Self {
        let t0 = std::time::Instant::now();
        let faces = vec![Face::new(font_bytes)];
        let atlases = vec![Atlas::new()];
        println!("[profile]   FontCache::new ({:?}): {:?}", tag, t0.elapsed());
        Self {
            tag,
            faces,
            fallback_for: FxHashMap::default(),
            emoji_fallback_for: FxHashMap::default(),
            face_by_key: FxHashMap::default(),
            shaped_cache: FxHashMap::default(),
            pos_scratch: Vec::new(),
            raster_cache: FxHashMap::default(),
            atlases,
            color_atlases: Vec::new(),
            pending_uploads: Vec::new(),
            textures_dirty: true,
        }
    }

    pub fn tag(&self) -> FontTag {
        self.tag
    }

    pub fn begin_frame(&mut self) {}

    pub fn atlas_count(&self) -> usize {
        self.atlases.len()
    }

    pub fn atlas(&self, index: usize) -> &Atlas {
        &self.atlases[index]
    }

    pub fn atlas_mut(&mut self, index: usize) -> &mut Atlas {
        &mut self.atlases[index]
    }

    pub fn atlas_texture_id(&self, index: usize) -> u32 {
        self.atlases[index].texture_id()
    }

    pub fn color_atlas_count(&self) -> usize {
        self.color_atlases.len()
    }

    pub fn color_atlas(&self, index: usize) -> &Atlas {
        &self.color_atlases[index]
    }

    pub fn color_atlas_mut(&mut self, index: usize) -> &mut Atlas {
        &mut self.color_atlases[index]
    }

    pub fn color_atlas_texture_id(&self, index: usize) -> u32 {
        self.color_atlases[index].texture_id()
    }

    pub fn take_pending_uploads(&mut self) -> Vec<AtlasUpload> {
        std::mem::take(&mut self.pending_uploads)
    }

    pub fn needs_texture_update(&self) -> bool {
        self.textures_dirty
    }

    pub fn mark_textures_clean(&mut self) {
        self.textures_dirty = false;
    }

    pub fn mark_backend_lost(&mut self) {
        self.pending_uploads.clear();
        for atlas in self.atlases.iter_mut().chain(self.color_atlases.iter_mut()) {
            atlas.set_texture_id(0);
        }
        self.textures_dirty = true;
    }

    pub fn line_height(&mut self, font_size: f32) -> f32 {
        self.faces[PRIMARY_FACE.0 as usize].line_height(quantize_px_value(font_size))
    }

    /// Per-char advance widths for `text`, shaped as a single *logical* run so
    /// kerning, ligatures, and contextual shaping match exactly what `run_from_text`
    /// renders. Each glyph's advance lands on the first char of its cluster (others
    /// get 0), and tab stops are honored. Cumulated left-to-right these give caret
    /// x positions that line up with the drawn glyphs.
    ///
    /// Logical order (no BiDi reordering): correct for LTR text and for the editor's
    /// logical caret model; mixed-direction caret geometry remains a known gap.
    pub fn char_advances(&mut self, size: f32, text: &str, out: &mut Vec<f32>) {
        out.clear();
        out.resize(text.chars().count(), 0.0);
        if out.is_empty() {
            return;
        }
        let qpx = quantize_px_value(size);
        let tab_px = qpx * 4.0;
        let mut tmp: Vec<ShapedGlyph> = Vec::new();
        let mut pen_x = 0.0f32;
        let mut seg_byte = 0usize;
        // Monotonic byte->char cursor (clusters are emitted in non-decreasing byte
        // order under logical shaping), advanced just before each assignment.
        let mut cur_byte = 0usize;
        let mut cur_char = 0usize;

        for (i, part) in text.split('\t').enumerate() {
            if i > 0 {
                let raw = tab_px - (pen_x % tab_px);
                let adv = if raw <= 0.0 { tab_px } else { raw };
                let tab_byte = seg_byte - 1;
                while cur_byte < tab_byte {
                    cur_byte += text[cur_byte..].chars().next().map_or(1, char::len_utf8);
                    cur_char += 1;
                }
                out[cur_char] += adv;
                pen_x += adv;
            }
            if !part.is_empty() {
                for (start, end, face) in self.face_runs(part) {
                    tmp.clear();
                    self.faces[face.0 as usize].shape_into(&part[start..end], qpx, false, &mut tmp);
                    for g in &tmp {
                        let abs_byte = seg_byte + start + g.cluster;
                        while cur_byte < abs_byte {
                            cur_byte += text[cur_byte..].chars().next().map_or(1, char::len_utf8);
                            cur_char += 1;
                        }
                        out[cur_char] += g.advance;
                        pen_x += g.advance;
                    }
                }
            }
            seg_byte += part.len() + 1;
        }
    }

    pub fn get_text_size(&mut self, size: f32, text: &str, length: usize) -> (f32, f32) {
        let length = clamp_to_char_boundary(text, length.min(text.len()));
        let qpx = quantize_px_value(size);
        let px_q = quantize_px_key(size);
        let tab_px = qpx * 4.0;

        let mut scratch = std::mem::take(&mut self.pos_scratch);
        self.shaped_items_into(&text[..length], qpx, px_q, &mut scratch);
        let mut pen_x = 0.0f32;
        for item in &scratch {
            match *item {
                CachedItem::Tab { .. } => {
                    let raw = tab_px - (pen_x % tab_px);
                    pen_x += if raw <= 0.0 { tab_px } else { raw };
                }
                CachedItem::Glyph { glyph, .. } => pen_x += glyph.advance,
            }
        }
        self.pos_scratch = scratch;
        (pen_x, self.faces[PRIMARY_FACE.0 as usize].line_height(qpx))
    }

    pub fn get_cursor_position(&mut self, size: f32, text: &str, cursorx: f32) -> (f32, usize) {
        let qpx = quantize_px_value(size);
        let px_q = quantize_px_key(size);
        let tab_px = qpx * 4.0;

        let mut scratch = std::mem::take(&mut self.pos_scratch);
        self.shaped_items_into(text, qpx, px_q, &mut scratch);
        let mut pen_x = 0.0f32;
        let mut hit = None;
        for item in &scratch {
            let (advance, cluster) = match *item {
                CachedItem::Tab { cluster } => {
                    let raw = tab_px - (pen_x % tab_px);
                    (if raw <= 0.0 { tab_px } else { raw }, cluster)
                }
                CachedItem::Glyph { glyph, .. } => (glyph.advance, glyph.cluster),
            };
            if hit.is_none() && cursorx <= pen_x + advance / 2.0 {
                hit = Some((pen_x, cluster));
            }
            pen_x += advance;
        }
        self.pos_scratch = scratch;
        hit.unwrap_or((pen_x, text.len()))
    }

    pub fn run_from_text(
        &mut self,
        size: f32,
        text: &str,
        length: usize,
        _dpi_scale: f32,
    ) -> TextRun {
        let mut pieces = Vec::new();
        let dim = self.build_run(size, text, length, RasterFlags::default(), &mut pieces);
        TextRun { pieces, dim }
    }

    pub fn run_from_text_into(
        &mut self,
        size: f32,
        text: &str,
        length: usize,
        _dpi_scale: f32,
        pieces: &mut Vec<TextPiece>,
    ) -> (f32, f32) {
        self.build_run(size, text, length, RasterFlags::default(), pieces)
    }

    /// Position + rasterize a run into `pieces`. Shaping (with fallback) is served
    /// from the shaped-run cache. Returns (width, line height).
    fn build_run(
        &mut self,
        size: f32,
        text: &str,
        length: usize,
        flags: RasterFlags,
        pieces: &mut Vec<TextPiece>,
    ) -> (f32, f32) {
        let length = clamp_to_char_boundary(text, length.min(text.len()));
        let qpx = quantize_px_value(size);
        let px_q = quantize_px_key(size);
        let tab_px = qpx * 4.0;

        let mut scratch = std::mem::take(&mut self.pos_scratch);
        self.shaped_items_into(&text[..length], qpx, px_q, &mut scratch);

        pieces.clear();
        if pieces.capacity() < scratch.len() {
            pieces.reserve(scratch.len() - pieces.capacity());
        }
        let mut pen_x = 0.0f32;
        for item in &scratch {
            match *item {
                CachedItem::Tab { .. } => {
                    let raw = tab_px - (pen_x % tab_px);
                    pen_x += if raw <= 0.0 { tab_px } else { raw };
                }
                CachedItem::Glyph { face, glyph } => {
                    let entry = self.raster_entry(face, glyph.glyph_id, qpx, px_q, flags);
                    pieces.push(TextPiece {
                        atlas_index: entry.region.atlas_index,
                        subrect_px: entry.region,
                        advance: glyph.advance,
                        cluster: glyph.cluster,
                        offset_px: (
                            pen_x + glyph.x_offset + entry.xoff,
                            entry.yoff - glyph.y_offset,
                        ),
                        color: entry.color,
                    });
                    pen_x += glyph.advance;
                }
            }
        }
        self.pos_scratch = scratch;
        (pen_x, self.faces[PRIMARY_FACE.0 as usize].line_height(qpx))
    }

    /// Ensure the shaped items for `(text, px)` are cached, then copy them into
    /// `out`. The copy frees the cache borrow so the caller can rasterize freely.
    fn shaped_items_into(&mut self, text: &str, qpx: f32, px_q: u16, out: &mut Vec<CachedItem>) {
        let key = run_hash(px_q, text);
        let hit = self
            .shaped_cache
            .get(&key)
            .is_some_and(|(cached_text, _)| cached_text == text);
        if !hit {
            let items = self.shape_run(text, qpx);
            if self.shaped_cache.len() >= SHAPED_CACHE_CAP {
                self.shaped_cache.clear();
            }
            self.shaped_cache.insert(key, (text.to_string(), items));
        }
        out.clear();
        if let Some((_, items)) = self.shaped_cache.get(&key) {
            out.extend_from_slice(items);
        }
    }

    /// Itemize `text` across faces (fallback) and shape each sub-run. Produces
    /// position-independent items (glyphs carry their `FaceId`) plus tab markers.
    fn shape_run(&mut self, text: &str, px: f32) -> Vec<CachedItem> {
        let mut items = Vec::new();
        let mut tmp: Vec<ShapedGlyph> = Vec::new();
        let mut seg_start = 0usize;
        for (i, part) in text.split('\t').enumerate() {
            if i > 0 {
                items.push(CachedItem::Tab {
                    cluster: seg_start.saturating_sub(1),
                });
            }
            if !part.is_empty() {
                self.shape_segment(part, seg_start, px, &mut tmp, &mut items);
            }
            seg_start += part.len() + 1;
        }
        items
    }

    /// Lay out a tab-free segment with the Unicode Bidirectional Algorithm: split
    /// into directional level runs, emit them in *visual* (left-to-right) order,
    /// and within each run itemize by face and shape with the run's direction.
    /// Glyphs still carry logical source byte offsets via `cluster`.
    fn shape_segment(
        &mut self,
        part: &str,
        base: usize,
        px: f32,
        tmp: &mut Vec<ShapedGlyph>,
        items: &mut Vec<CachedItem>,
    ) {
        let bidi = unicode_bidi::ParagraphBidiInfo::new(part, None);
        if !bidi.has_rtl() {
            // Pure LTR fast path: one visual run covering the whole segment.
            self.shape_directional(part, 0..part.len(), base, false, px, tmp, items);
            return;
        }
        let (levels, runs) = bidi.visual_runs(0..part.len());
        for run in runs {
            let rtl = levels[run.start].is_rtl();
            self.shape_directional(part, run, base, rtl, px, tmp, items);
        }
    }

    /// Itemize one uniform-direction byte range by face and shape each sub-run.
    /// For an RTL run, face sub-runs are emitted in reverse (visual) order.
    fn shape_directional(
        &mut self,
        part: &str,
        run: std::ops::Range<usize>,
        base: usize,
        rtl: bool,
        px: f32,
        tmp: &mut Vec<ShapedGlyph>,
        items: &mut Vec<CachedItem>,
    ) {
        let mut face_runs = self.face_runs(&part[run.clone()]);
        if rtl {
            face_runs.reverse();
        }
        for (start, end, face) in face_runs {
            let abs_start = run.start + start;
            let abs_end = run.start + end;
            tmp.clear();
            self.faces[face.0 as usize].shape_into(&part[abs_start..abs_end], px, rtl, tmp);
            for g in tmp.iter() {
                items.push(CachedItem::Glyph {
                    face,
                    glyph: ShapedGlyph {
                        cluster: base + abs_start + g.cluster,
                        ..*g
                    },
                });
            }
        }
    }

    /// Split `sub` into maximal same-face byte ranges (logical order). A codepoint
    /// immediately followed by U+FE0F (emoji variation selector) is forced onto a
    /// color emoji face even if the primary covers its text form.
    fn face_runs(&mut self, sub: &str) -> Vec<(usize, usize, FaceId)> {
        let chars: Vec<(usize, char)> = sub.char_indices().collect();

        let mut faces: Vec<FaceId> = Vec::with_capacity(chars.len());
        for (i, &(_, ch)) in chars.iter().enumerate() {
            let face = if ch == EMOJI_VARIATION_SELECTOR {
                faces.last().copied().unwrap_or(PRIMARY_FACE)
            } else if chars
                .get(i + 1)
                .is_some_and(|&(_, next)| next == EMOJI_VARIATION_SELECTOR)
            {
                self.resolve_emoji_face(ch)
            } else {
                self.resolve_face(ch)
            };
            faces.push(face);
        }

        let mut runs = Vec::new();
        let mut run_start = 0usize;
        for i in 0..chars.len() {
            if i + 1 == chars.len() || faces[i + 1] != faces[i] {
                let start_byte = chars[run_start].0;
                let end_byte = chars.get(i + 1).map_or(sub.len(), |&(b, _)| b);
                runs.push((start_byte, end_byte, faces[i]));
                run_start = i + 1;
            }
        }
        runs
    }

    /// Resolve which face renders `c`: the primary if it covers it, else a
    /// lazily-loaded OS fallback (cached), else the primary (`.notdef`).
    fn resolve_face(&mut self, c: char) -> FaceId {
        if self.faces[PRIMARY_FACE.0 as usize].covers(c) {
            return PRIMARY_FACE;
        }
        if let Some(&cached) = self.fallback_for.get(&c) {
            return cached.unwrap_or(PRIMARY_FACE);
        }
        let resolved = self.load_fallback(c);
        self.fallback_for.insert(c, resolved);
        resolved.unwrap_or(PRIMARY_FACE)
    }

    /// Resolve a codepoint that requested emoji presentation (trailing U+FE0F) to a
    /// color emoji face, falling back to normal resolution if none is found.
    fn resolve_emoji_face(&mut self, c: char) -> FaceId {
        if let Some(&cached) = self.emoji_fallback_for.get(&c) {
            return match cached {
                Some(id) => id,
                None => self.resolve_face(c),
            };
        }
        let resolved = self.intern_fallback(super::font_fallback::locate_emoji(c), c);
        self.emoji_fallback_for.insert(c, resolved);
        match resolved {
            Some(id) => id,
            None => self.resolve_face(c),
        }
    }

    fn load_fallback(&mut self, c: char) -> Option<FaceId> {
        self.intern_fallback(super::font_fallback::locate(c), c)
    }

    /// Intern a located fallback font as a face (deduped by path+index), verifying
    /// it actually covers `c`. Returns the (possibly reused) face id.
    fn intern_fallback(
        &mut self,
        loaded: Option<super::font_fallback::LoadedFont>,
        c: char,
    ) -> Option<FaceId> {
        let loaded = loaded?;
        let key = (loaded.key, loaded.index);
        // Dedup before touching the file: many CJK codepoints resolve to the same
        // system fallback face, so this fast path keeps the typing hot path from
        // re-reading (and re-parsing) a multi-MB font once per new codepoint.
        if let Some(&id) = self.face_by_key.get(&key) {
            return Some(id);
        }
        // Only a genuine new face pays the read — locators that already had the
        // bytes (macOS) hand them over; others (Windows) defer the read to here.
        let bytes = match loaded.bytes {
            Some(bytes) => bytes,
            None => std::fs::read(&key.0).ok()?,
        };
        let face = Face::new_with_index(&bytes, loaded.index);
        if !face.covers(c) {
            return None;
        }
        let id = FaceId(self.faces.len() as u16);
        self.faces.push(face);
        self.face_by_key.insert(key, id);
        Some(id)
    }

    fn raster_entry(
        &mut self,
        face_id: FaceId,
        glyph_id: u16,
        px: f32,
        px_q: u16,
        flags: RasterFlags,
    ) -> RasterEntry {
        let key = glyph_key(face_id, glyph_id, px_q, flags);
        if let Some(entry) = self.raster_cache.get(&key) {
            return *entry;
        }

        let _ = flags;
        let raster = self.faces[face_id.0 as usize]
            .rasterize(glyph_id, px)
            .unwrap_or_else(RasterizedGlyph::empty);

        let region = self.place_raster(raster.width, raster.height, &raster.bitmap, raster.color);
        let entry = RasterEntry {
            region,
            xoff: raster.xoff,
            yoff: raster.yoff,
            color: raster.color,
        };
        self.raster_cache.insert(key, entry);
        entry
    }

    /// Pack a glyph bitmap into the alpha or color atlases (per `color`) and queue
    /// a texture upload. Returns the atlas region (index is into the matching vec).
    fn place_raster(
        &mut self,
        width: usize,
        height: usize,
        bitmap: &[u8],
        color: bool,
    ) -> AtlasRegion {
        if width == 0 || height == 0 {
            return AtlasRegion {
                atlas_index: 0,
                x: 0,
                y: 0,
                width,
                height,
            };
        }
        if width + ATLAS_PADDING * 2 > ATLAS_SIZE || height + ATLAS_PADDING * 2 > ATLAS_SIZE {
            panic!(
                "single glyph raster ({}x{}) is larger than the {}x{} atlas",
                width, height, ATLAS_SIZE, ATLAS_SIZE
            );
        }

        let atlases = if color {
            &mut self.color_atlases
        } else {
            &mut self.atlases
        };

        for (atlas_index, atlas) in atlases.iter_mut().enumerate() {
            if let Some((x, y)) = atlas.allocate(width, height) {
                atlas.write_region(x, y, width, height, bitmap);
                self.pending_uploads.push(AtlasUpload {
                    atlas_index,
                    x,
                    y,
                    width,
                    height,
                    data: bitmap.to_vec(),
                    color,
                });
                self.textures_dirty = true;
                return AtlasRegion {
                    atlas_index,
                    x,
                    y,
                    width,
                    height,
                };
            }
        }

        atlases.push(if color {
            Atlas::new_color()
        } else {
            Atlas::new()
        });
        let atlas_index = atlases.len() - 1;
        let atlas = atlases.last_mut().unwrap();
        let (x, y) = atlas.allocate(width, height).unwrap();
        atlas.write_region(x, y, width, height, bitmap);
        self.pending_uploads.push(AtlasUpload {
            atlas_index,
            x,
            y,
            width,
            height,
            data: bitmap.to_vec(),
            color,
        });
        self.textures_dirty = true;
        AtlasRegion {
            atlas_index,
            x,
            y,
            width,
            height,
        }
    }

    #[cfg(test)]
    fn new_for_test() -> Self {
        Self::new_with_tag(
            FontTag::Main,
            include_bytes!("../../assets/NotoSans-Regular.ttf"),
        )
    }
}

/// Pack the raster cache key into a single `u64`:
/// `[ flags:8 | face_id:16 | px_size:16 | glyph_id:16 ]`.
#[inline]
fn glyph_key(face_id: FaceId, glyph_id: u16, px_q: u16, flags: RasterFlags) -> u64 {
    (glyph_id as u64)
        | ((px_q as u64) << 16)
        | ((face_id.0 as u64) << 32)
        | (((flags.0 as u8) as u64) << 48)
}

/// Hash `(px, text)` for the shaped-run cache. Stored alongside the source text
/// so collisions are detected and simply rebuilt (no alloc on cache hits).
#[inline]
fn run_hash(px_q: u16, text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = rustc_hash::FxHasher::default();
    px_q.hash(&mut hasher);
    text.hash(&mut hasher);
    hasher.finish()
}

/// Quantize a pixel size to 0.5px steps for both rasterization and the cache key.
#[inline]
fn quantize_px_value(size: f32) -> f32 {
    (size * 2.0).round() / 2.0
}

#[inline]
fn quantize_px_key(size: f32) -> u16 {
    ((size * 2.0).round() as i32).clamp(0, u16::MAX as i32) as u16
}

fn clamp_to_char_boundary(text: &str, mut length: usize) -> usize {
    while length > 0 && !text.is_char_boundary(length) {
        length -= 1;
    }
    length
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_allocations_do_not_overlap() {
        let mut atlas = Atlas::new();
        let a = atlas.allocate(10, 10).unwrap();
        let b = atlas.allocate(10, 10).unwrap();
        assert_ne!(a, b);
        assert!(b.0 >= a.0 + 10 + ATLAS_PADDING);
    }

    #[test]
    fn text_run_pieces_map_to_source_clusters() {
        let mut cache = FontCache::new_for_test();
        // "aé" - 'a' at byte 0, 'é' (U+00E9, 2 bytes) at byte 1.
        let run = cache.run_from_text(20.0, "a\u{00e9}", 3, 1.0);
        assert_eq!(run.pieces.len(), 2);
        assert_eq!(run.pieces[0].cluster, 0);
        assert_eq!(run.pieces[1].cluster, 1);
    }

    #[test]
    fn tabs_align_to_stops() {
        let mut cache = FontCache::new_for_test();
        // A single 'a' is narrower than one tab stop (4 * size), so "a\t" must
        // round up to exactly one tab stop.
        let tab_stop = 20.0 * 4.0;
        let (w, _) = cache.get_text_size(20.0, "a\t", 2);
        assert!((w - tab_stop).abs() < 0.5, "tab width {w} != {tab_stop}");
    }

    #[test]
    fn repeated_text_run_reuses_raster_cache() {
        let mut cache = FontCache::new_for_test();
        let first = cache.run_from_text(20.0, "abc", 3, 1.0);
        cache.take_pending_uploads();
        let second = cache.run_from_text(20.0, "abc", 3, 1.0);
        assert_eq!(second.pieces.len(), first.pieces.len());
        // No new glyphs rasterized -> no new uploads.
        assert!(cache.take_pending_uploads().is_empty());
    }

    #[test]
    fn run_width_matches_summed_advances() {
        let mut cache = FontCache::new_for_test();
        let run = cache.run_from_text(20.0, "abc", 3, 1.0);
        let summed = run.pieces.iter().map(|piece| piece.advance).sum::<f32>();
        assert!((run.dim.0 - summed).abs() < 0.01);
    }

    #[test]
    fn ascii_uses_primary_face_only() {
        let mut cache = FontCache::new_for_test();
        assert_eq!(cache.resolve_face('a'), PRIMARY_FACE);
        assert_eq!(cache.faces.len(), 1, "ASCII must not load fallback faces");
    }

    // NotoSans-Regular covers Cyrillic, so it stays on the primary face - a good
    // check that itemization does not needlessly fall back.
    #[test]
    fn cyrillic_stays_on_primary_face() {
        let mut cache = FontCache::new_for_test();
        assert_eq!(cache.resolve_face('Я'), PRIMARY_FACE);
    }

    // Exercises the real OS locator: a CJK codepoint the primary lacks must load a
    // fallback face and rasterize to a real (non-empty) glyph.
    #[cfg(target_os = "macos")]
    #[test]
    fn cjk_resolves_to_os_fallback_face() {
        let mut cache = FontCache::new_for_test();
        let face = cache.resolve_face('中');
        assert_ne!(face, PRIMARY_FACE, "CJK should resolve to a fallback face");
        assert_eq!(cache.faces.len(), 2, "exactly one fallback face loaded");
        // Resolving again must reuse the cached resolution, not reload.
        cache.resolve_face('中');
        assert_eq!(cache.faces.len(), 2);

        let run = cache.run_from_text(40.0, "中", "中".len(), 1.0);
        assert_eq!(run.pieces.len(), 1);
        assert!(
            run.pieces[0].subrect_px.width > 0,
            "fallback glyph should rasterize to a non-empty bitmap"
        );
    }

    // Color emoji must resolve to a fallback face and rasterize as a color (RGBA)
    // glyph routed into the color atlases.
    #[cfg(target_os = "macos")]
    #[test]
    fn emoji_rasterizes_as_color_glyph() {
        let mut cache = FontCache::new_for_test();
        let run = cache.run_from_text(40.0, "😀", "😀".len(), 1.0);
        assert_eq!(run.pieces.len(), 1);
        let piece = run.pieces[0];
        assert!(piece.color, "emoji should be a color glyph");
        assert!(piece.subrect_px.width > 0 && piece.subrect_px.height > 0);
        assert!(cache.color_atlas_count() >= 1, "a color atlas was created");
    }

    // A pure-RTL run must be emitted in visual (left-to-right) order, i.e. starting
    // from the logically-last character, so piece clusters run descending.
    #[cfg(target_os = "macos")]
    #[test]
    fn rtl_run_is_visually_reordered() {
        let mut cache = FontCache::new_for_test();
        let hebrew = "\u{05D0}\u{05D1}\u{05D2}"; // א ב ג, logical clusters 0,2,4
        let run = cache.run_from_text(20.0, hebrew, hebrew.len(), 1.0);
        assert!(run.pieces.len() >= 2);
        assert!(
            run.pieces.first().unwrap().cluster > run.pieces.last().unwrap().cluster,
            "RTL run should be reordered: clusters {:?}",
            run.pieces.iter().map(|p| p.cluster).collect::<Vec<_>>()
        );
    }

    // Caret geometry must match the rendered run: per-char advances (with kerning,
    // ligatures, clustering) sum to the same width get_text_size reports, so the
    // caret can't drift off the glyphs. Includes a kerning pair, a ligature, and an
    // emoji cluster.
    #[test]
    fn char_advances_sum_matches_run_width() {
        let mut cache = FontCache::new_for_test();
        let text = "AV To fi a\u{1F44D}\u{1F3FD}b";
        let mut adv = Vec::new();
        cache.char_advances(20.0, text, &mut adv);
        assert_eq!(adv.len(), text.chars().count());
        let sum: f32 = adv.iter().sum();
        let width = cache.get_text_size(20.0, text, text.len()).0;
        assert!(
            (sum - width).abs() < 0.5,
            "advances sum {sum} != run width {width}"
        );
        // The skin-tone modifier (interior of the emoji cluster) carries no advance -
        // only guaranteed where a color emoji font with ligature substitution is
        // available for fallback (see the other `target_os = "macos"` emoji tests).
        #[cfg(target_os = "macos")]
        {
            let modifier_char_idx = "AV To fi a\u{1F44D}".chars().count();
            assert_eq!(adv[modifier_char_idx], 0.0);
        }
    }

    // LTR text stays in logical order (ascending clusters).
    #[test]
    fn ltr_run_stays_in_logical_order() {
        let mut cache = FontCache::new_for_test();
        let run = cache.run_from_text(20.0, "abc", 3, 1.0);
        let clusters: Vec<usize> = run.pieces.iter().map(|p| p.cluster).collect();
        assert!(
            clusters.windows(2).all(|w| w[0] <= w[1]),
            "clusters {clusters:?}"
        );
    }

    // U+2764 has a monochrome text form the primary may cover, but a trailing
    // U+FE0F must force color emoji presentation.
    #[cfg(target_os = "macos")]
    #[test]
    fn vs16_forces_color_emoji_presentation() {
        let mut cache = FontCache::new_for_test();
        let heart = "\u{2764}\u{FE0F}";
        let run = cache.run_from_text(40.0, heart, heart.len(), 1.0);
        assert!(
            run.pieces.iter().any(|p| p.color),
            "❤\u{FE0F} should render as a color emoji glyph"
        );
    }

    // Regression: Arabic contextual forms (cursive joining) must rasterize. fontdue
    // produced 0x0 for GeezaPro's joined glyphs; skrifa renders them. The medial
    // letters of العربية are the ones that previously came out empty.
    #[cfg(target_os = "macos")]
    #[test]
    fn arabic_contextual_glyphs_rasterize() {
        let mut cache = FontCache::new_for_test();
        let word = "العربية";
        let run = cache.run_from_text(40.0, word, word.len(), 1.0);
        let rendered = run
            .pieces
            .iter()
            .filter(|p| p.subrect_px.width > 0 && p.subrect_px.height > 0)
            .count();
        assert!(
            rendered >= run.pieces.len() - 1,
            "most Arabic glyphs should rasterize (got {rendered}/{} non-empty)",
            run.pieces.len()
        );
    }
}
