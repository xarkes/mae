use std::collections::HashMap;

pub const ATLAS_SIZE: usize = 1024;
const ATLAS_PADDING: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontTag {
    Main,
    Icon,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RasterFlags(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextStyleKey {
    pub font_tag: FontTag,
    pub size_x2: u32,
    pub dpi_x100: u32,
    pub raster_flags: RasterFlags,
}

impl TextStyleKey {
    pub fn new(font_tag: FontTag, size: f32, dpi_scale: f32, raster_flags: RasterFlags) -> Self {
        Self {
            font_tag,
            size_x2: quantize_size(size).0,
            dpi_x100: (dpi_scale.max(0.01) * 100.0).round() as u32,
            raster_flags,
        }
    }

    pub fn quantized_size(self) -> f32 {
        self.size_x2 as f32 / 2.0
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Glyph {
    pub width: usize,
    pub height: usize,
    pub advance: f32,
    pub xoff: f32,
    pub yoff: f32,
}

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

#[derive(Clone, Debug)]
pub struct TextPiece {
    pub texture_id: u32,
    pub atlas_index: usize,
    pub subrect_px: AtlasRegion,
    pub advance: f32,
    pub decode_len: usize,
    pub offset_px: (f32, f32),
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
}

#[derive(Clone, Debug)]
pub struct Atlas {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
    next_x: usize,
    next_y: usize,
    row_height: usize,
    texture_id: u32,
}

impl Atlas {
    pub fn new() -> Self {
        Self {
            data: vec![0; ATLAS_SIZE * ATLAS_SIZE],
            width: ATLAS_SIZE,
            height: ATLAS_SIZE,
            next_x: ATLAS_PADDING,
            next_y: ATLAS_PADDING,
            row_height: 0,
            texture_id: 0,
        }
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
        debug_assert_eq!(data.len(), width * height);
        for row in 0..height {
            let dst = (y + row) * self.width + x;
            let src = row * width;
            self.data[dst..dst + width].copy_from_slice(&data[src..src + width]);
        }
    }
}

pub trait FontProvider {
    fn has_glyph(&self, glyph: char) -> bool;
    fn rasterize(&mut self, glyph: char, size: f32, flags: RasterFlags) -> Option<RasterizedGlyph>;
    fn line_height(&mut self, size: f32) -> f32;
}

pub struct RasterizedGlyph {
    pub glyph: Glyph,
    pub bitmap: Vec<u8>,
}

#[cfg(feature = "fontdue")]
struct FontdueProvider {
    font: fontdue::Font,
}

#[cfg(feature = "fontdue")]
impl FontdueProvider {
    fn new(font_bytes: &[u8]) -> Self {
        let t0 = std::time::Instant::now();
        let font = fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default()).unwrap();
        println!(
            "[profile]   fontdue::Font::from_bytes: {:?} (input: {} KB)",
            t0.elapsed(),
            font_bytes.len() / 1024
        );
        Self { font }
    }
}

#[cfg(feature = "fontdue")]
impl FontProvider for FontdueProvider {
    fn has_glyph(&self, glyph: char) -> bool {
        self.font.has_glyph(glyph)
    }

    fn rasterize(
        &mut self,
        glyph: char,
        size: f32,
        _flags: RasterFlags,
    ) -> Option<RasterizedGlyph> {
        if !self.has_glyph(glyph) {
            return None;
        }
        let (metrics, bitmap) = self.font.rasterize(glyph, size);
        Some(RasterizedGlyph {
            glyph: Glyph {
                width: metrics.width,
                height: metrics.height,
                advance: metrics.advance_width,
                xoff: metrics.xmin as f32,
                yoff: -(metrics.height as f32 + metrics.ymin as f32),
            },
            bitmap,
        })
    }

    fn line_height(&mut self, size: f32) -> f32 {
        self.font
            .horizontal_line_metrics(size)
            .map(|m| m.new_line_size)
            .unwrap_or(size * 1.2)
    }
}

#[cfg(all(feature = "freetype", not(feature = "fontdue"), target_os = "linux"))]
struct FreeTypeProvider {
    _library: freetype::Library,
    face: freetype::Face,
}

#[cfg(all(feature = "freetype", not(feature = "fontdue"), target_os = "linux"))]
impl FreeTypeProvider {
    fn new(font_bytes: &[u8]) -> Self {
        let t0 = std::time::Instant::now();
        let library = freetype::Library::init().expect("Failed to initialize FreeType");
        let face = library
            .new_memory_face(font_bytes.to_vec(), 0)
            .expect("Failed to load font face");
        println!(
            "[profile]   freetype::Face::new: {:?} (input: {} KB)",
            t0.elapsed(),
            font_bytes.len() / 1024
        );
        Self {
            _library: library,
            face,
        }
    }
}

#[cfg(all(feature = "freetype", not(feature = "fontdue"), target_os = "linux"))]
impl FontProvider for FreeTypeProvider {
    fn has_glyph(&self, glyph: char) -> bool {
        self.face.get_char_index(glyph as usize).is_some()
    }

    fn rasterize(
        &mut self,
        glyph: char,
        size: f32,
        _flags: RasterFlags,
    ) -> Option<RasterizedGlyph> {
        use freetype::face::LoadFlag;

        let pixel_size = size.round().max(1.0) as u32;
        self.face.set_pixel_sizes(0, pixel_size).ok()?;
        let glyph_index = self.face.get_char_index(glyph as usize)?;
        self.face.load_glyph(glyph_index, LoadFlag::RENDER).ok()?;

        let glyph_slot = self.face.glyph();
        let bitmap = glyph_slot.bitmap();
        let width = bitmap.width() as usize;
        let height = bitmap.rows() as usize;
        let mut data = vec![0u8; width * height];
        let pitch = bitmap.pitch().unsigned_abs() as usize;
        let buffer = bitmap.buffer();
        for y in 0..height {
            let src = y * pitch;
            let dst = y * width;
            if src + width <= buffer.len() {
                data[dst..dst + width].copy_from_slice(&buffer[src..src + width]);
            }
        }

        let metrics = glyph_slot.metrics();
        Some(RasterizedGlyph {
            glyph: Glyph {
                width,
                height,
                advance: (metrics.horiAdvance >> 6) as f32,
                xoff: glyph_slot.bitmap_left() as f32,
                yoff: -(height as f32 + (glyph_slot.bitmap_top() - height as i32) as f32),
            },
            bitmap: data,
        })
    }

    fn line_height(&mut self, size: f32) -> f32 {
        let pixel_size = size.round().max(1.0) as u32;
        let _ = self.face.set_pixel_sizes(0, pixel_size);
        self.face
            .size_metrics()
            .map(|m| (m.height >> 6) as f32)
            .unwrap_or(size * 1.2)
    }
}

struct FallbackProvider;

impl FontProvider for FallbackProvider {
    fn has_glyph(&self, _glyph: char) -> bool {
        true
    }

    fn rasterize(
        &mut self,
        _glyph: char,
        size: f32,
        _flags: RasterFlags,
    ) -> Option<RasterizedGlyph> {
        Some(RasterizedGlyph {
            glyph: Glyph {
                width: 0,
                height: 0,
                advance: size * 0.6,
                xoff: 0.0,
                yoff: 0.0,
            },
            bitmap: Vec::new(),
        })
    }

    fn line_height(&mut self, size: f32) -> f32 {
        size * 1.2
    }
}

#[derive(Clone, Debug)]
struct RasterEntry {
    glyph: Glyph,
    region: AtlasRegion,
}

pub struct FontCache {
    tag: FontTag,
    provider: Box<dyn FontProvider>,
    raster_cache: HashMap<(TextStyleKey, char), RasterEntry>,
    run_cache: HashMap<(TextStyleKey, String, usize), TextRun>,
    atlases: Vec<Atlas>,
    pending_uploads: Vec<AtlasUpload>,
}

impl FontCache {
    pub fn new(font_bytes: &[u8]) -> Self {
        Self::new_with_tag(FontTag::Main, font_bytes)
    }

    pub fn new_with_tag(tag: FontTag, font_bytes: &[u8]) -> Self {
        let t0 = std::time::Instant::now();
        let provider: Box<dyn FontProvider> = create_provider(font_bytes);
        let atlases = vec![Atlas::new()];
        println!("[profile]   FontCache::new ({:?}): {:?}", tag, t0.elapsed());
        Self {
            tag,
            provider,
            raster_cache: HashMap::new(),
            run_cache: HashMap::new(),
            atlases,
            pending_uploads: Vec::new(),
        }
    }

    pub fn tag(&self) -> FontTag {
        self.tag
    }

    pub fn begin_frame(&mut self) {
        self.run_cache.clear();
    }

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

    pub fn take_pending_uploads(&mut self) -> Vec<AtlasUpload> {
        std::mem::take(&mut self.pending_uploads)
    }

    pub fn mark_backend_lost(&mut self) {
        self.pending_uploads.clear();
        for atlas in &mut self.atlases {
            atlas.set_texture_id(0);
        }
        self.refresh_run_texture_ids();
    }

    pub fn line_height(&mut self, font_size: f32) -> f32 {
        self.provider.line_height(font_size)
    }

    pub fn get_text_size(&mut self, size: f32, text: &str, length: usize) -> (f32, f32) {
        self.run_from_text(size, text, length, 1.0).dim
    }

    pub fn get_cursor_position(&mut self, size: f32, text: &str, cursorx: f32) -> (f32, usize) {
        let run = self.run_from_text(size, text, text.len(), 1.0);
        let mut x = 0.0;
        let mut byte_idx = 0;
        for piece in &run.pieces {
            if cursorx > x + piece.advance / 2.0 {
                x += piece.advance;
                byte_idx += piece.decode_len;
            } else {
                break;
            }
        }
        (x, byte_idx)
    }

    pub fn run_from_text(
        &mut self,
        size: f32,
        text: &str,
        length: usize,
        dpi_scale: f32,
    ) -> TextRun {
        let style = TextStyleKey::new(self.tag, size, dpi_scale, RasterFlags::default());
        let length = clamp_to_char_boundary(text, length.min(text.len()));
        let cache_key = (style, text[..length].to_owned(), length);
        if let Some(run) = self.run_cache.get(&cache_key) {
            return run.clone();
        }

        let run = self.build_run(style, text, length.min(text.len()));
        self.run_cache.insert(cache_key, run.clone());
        run
    }

    fn build_run(&mut self, style: TextStyleKey, text: &str, length: usize) -> TextRun {
        let size = style.quantized_size();
        let mut x = 0.0;
        let mut pieces = Vec::new();
        let tab_size_px = size * 4.0;

        for (byte_idx, ch) in text.char_indices() {
            if byte_idx >= length {
                break;
            }

            if ch == '\t' {
                let advance = tab_size_px - (x % tab_size_px);
                x += if advance <= 0.0 { tab_size_px } else { advance };
                continue;
            }

            let entry = self.raster_entry(style, ch);
            let texture_id = self.atlas_texture_id(entry.region.atlas_index);
            pieces.push(TextPiece {
                texture_id,
                atlas_index: entry.region.atlas_index,
                subrect_px: entry.region,
                advance: entry.glyph.advance,
                decode_len: ch.len_utf8(),
                offset_px: (x + entry.glyph.xoff, entry.glyph.yoff),
            });
            x += entry.glyph.advance;
        }

        TextRun {
            pieces,
            dim: (x, self.line_height(size)),
        }
    }

    fn raster_entry(&mut self, style: TextStyleKey, glyph: char) -> RasterEntry {
        let glyph = if self.provider.has_glyph(glyph) {
            glyph
        } else {
            '?'
        };

        if let Some(entry) = self.raster_cache.get(&(style, glyph)) {
            return entry.clone();
        }

        let raster = self
            .provider
            .rasterize(glyph, style.quantized_size(), style.raster_flags)
            .unwrap_or_else(|| {
                self.provider
                    .rasterize('?', style.quantized_size(), style.raster_flags)
                    .unwrap_or(RasterizedGlyph {
                        glyph: Glyph::default(),
                        bitmap: Vec::new(),
                    })
            });

        let region = self.place_raster(raster.glyph.width, raster.glyph.height, &raster.bitmap);
        let entry = RasterEntry {
            glyph: raster.glyph,
            region,
        };
        self.raster_cache.insert((style, glyph), entry.clone());
        entry
    }

    fn place_raster(&mut self, width: usize, height: usize, bitmap: &[u8]) -> AtlasRegion {
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

        for (atlas_index, atlas) in self.atlases.iter_mut().enumerate() {
            if let Some((x, y)) = atlas.allocate(width, height) {
                atlas.write_region(x, y, width, height, bitmap);
                self.pending_uploads.push(AtlasUpload {
                    atlas_index,
                    x,
                    y,
                    width,
                    height,
                    data: bitmap.to_vec(),
                });
                return AtlasRegion {
                    atlas_index,
                    x,
                    y,
                    width,
                    height,
                };
            }
        }

        self.atlases.push(Atlas::new());
        let atlas_index = self.atlases.len() - 1;
        let atlas = self.atlases.last_mut().unwrap();
        let (x, y) = atlas.allocate(width, height).unwrap();
        atlas.write_region(x, y, width, height, bitmap);
        self.pending_uploads.push(AtlasUpload {
            atlas_index,
            x,
            y,
            width,
            height,
            data: bitmap.to_vec(),
        });
        AtlasRegion {
            atlas_index,
            x,
            y,
            width,
            height,
        }
    }

    pub fn refresh_run_texture_ids(&mut self) {
        let texture_ids: Vec<u32> = self.atlases.iter().map(Atlas::texture_id).collect();
        for run in self.run_cache.values_mut() {
            for piece in &mut run.pieces {
                piece.texture_id = texture_ids[piece.atlas_index];
            }
        }
    }

    #[cfg(test)]
    fn new_for_test() -> Self {
        Self {
            tag: FontTag::Main,
            provider: Box::new(FallbackProvider),
            raster_cache: HashMap::new(),
            run_cache: HashMap::new(),
            atlases: vec![Atlas::new()],
            pending_uploads: Vec::new(),
        }
    }
}

fn create_provider(_font_bytes: &[u8]) -> Box<dyn FontProvider> {
    #[cfg(feature = "fontdue")]
    {
        return Box::new(FontdueProvider::new(_font_bytes));
    }

    #[cfg(all(feature = "freetype", not(feature = "fontdue"), target_os = "linux"))]
    {
        return Box::new(FreeTypeProvider::new(_font_bytes));
    }

    #[allow(unreachable_code)]
    Box::new(FallbackProvider)
}

#[inline]
fn quantize_size(size: f32) -> (u32, f32) {
    let quantized = (size * 2.0).round() / 2.0;
    ((quantized * 2.0) as u32, quantized)
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
    fn text_run_preserves_utf8_decode_lengths() {
        let mut cache = FontCache::new_for_test();
        let run = cache.run_from_text(10.0, "a\u{00e9}", 3, 1.0);
        assert_eq!(run.pieces[0].decode_len, 1);
        assert_eq!(run.pieces[1].decode_len, 2);
    }

    #[test]
    fn tabs_align_to_stops() {
        let mut cache = FontCache::new_for_test();
        let run = cache.run_from_text(10.0, "a\tb", 3, 1.0);
        assert_eq!(run.dim.0, 46.0);
    }

    #[test]
    fn repeated_text_run_uses_frame_cache() {
        let mut cache = FontCache::new_for_test();
        let first = cache.run_from_text(10.0, "abc", 3, 1.0);
        let upload_count = cache.pending_uploads.len();
        let second = cache.run_from_text(10.0, "abc", 3, 1.0);
        assert_eq!(second.pieces.len(), first.pieces.len());
        assert_eq!(cache.pending_uploads.len(), upload_count);
    }

    #[test]
    fn run_width_matches_summed_advances() {
        let mut cache = FontCache::new_for_test();
        let run = cache.run_from_text(10.0, "abc", 3, 1.0);
        let summed = run.pieces.iter().map(|piece| piece.advance).sum::<f32>();
        assert_eq!(run.dim.0, summed);
    }
}
