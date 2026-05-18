// Font caching and rasterization with pluggable backends.
//
// Backends:
//   - fontdue (default): Pure Rust, no system dependencies
//   - freetype: System library, better performance and memory usage
//
// Use `--features freetype --no-default-features` to use freetype instead of fontdue.

#[cfg(any(feature = "fontdue", all(feature = "freetype", target_os = "linux")))]
use std::collections::HashMap;
#[cfg(any(feature = "fontdue", all(feature = "freetype", target_os = "linux")))]
use std::num::NonZeroUsize;

#[cfg(any(feature = "fontdue", all(feature = "freetype", target_os = "linux")))]
use lru::LruCache;

#[cfg(any(feature = "fontdue", all(feature = "freetype", target_os = "linux")))]
const CACHE_GLYPH_COUNT: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(512) };

/// Quantize font size to avoid floating point precision issues.
/// Rounds to nearest 0.5pt, returns a key suitable for HashMap lookup.
#[inline]
#[cfg(any(feature = "fontdue", all(feature = "freetype", target_os = "linux")))]
fn quantize_size(size: f32) -> (u32, f32) {
    // Round to nearest 0.5 (multiply by 2, round, divide by 2)
    let quantized = (size * 2.0).round() / 2.0;
    let key = (quantized * 2.0) as u32; // Unique key for each 0.5 increment
    (key, quantized)
}

const ATLAS_WIDTH: usize = 1024;

// ============================================================================
// Common types used by all backends
// ============================================================================

#[derive(Clone, Copy, Debug, Default)]
pub struct Glyph {
    pub width: usize,
    pub height: usize,
    pub advance: f32,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub xoff: f32,
    pub yoff: f32,
}

pub struct Atlas {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
    next_x: usize,
    next_y: usize,
    cur_max_height: usize,
}

/// Metrics from rasterization, used by Atlas::add_glyph
#[cfg(any(feature = "fontdue", all(feature = "freetype", target_os = "linux")))]
struct RasterMetrics {
    width: usize,
    height: usize,
    advance_width: f32,
    xmin: i32,
    ymin: i32,
}

impl Atlas {
    pub fn new() -> Self {
        Atlas {
            data: vec![0; ATLAS_WIDTH * ATLAS_WIDTH],
            width: ATLAS_WIDTH,
            height: ATLAS_WIDTH,
            next_x: 0,
            next_y: 0,
            cur_max_height: 0,
        }
    }

    /// Add a glyph to the current atlas
    #[cfg(any(feature = "fontdue", all(feature = "freetype", target_os = "linux")))]
    fn add_glyph(&mut self, metrics: RasterMetrics, bitmap: Vec<u8>) -> Glyph {
        if self.next_y >= self.height {
            // TODO(xarkes): Implement atlas eviction
            panic!("Full atlas is not handled yet");
        }
        if self.next_x + metrics.width >= self.width {
            self.next_x = 0;
            self.next_y += self.cur_max_height;
            self.cur_max_height = 0;
        }

        // Copy the square rasterized glyph in our atlas (non contiguous)
        for y in 0..metrics.height {
            let dst = &mut self.data[self.next_x + y * self.width + self.next_y * self.width
                ..self.next_x + y * self.width + self.next_y * self.width + metrics.width];
            let data = &bitmap[y * metrics.width..y * metrics.width + metrics.width];
            dst.copy_from_slice(data);
        }

        let glyph = Glyph {
            width: metrics.width,
            height: metrics.height,
            advance: metrics.advance_width,
            x0: self.next_x as f32 / self.width as f32,
            y0: self.next_y as f32 / self.height as f32,
            x1: (self.next_x as f32 + metrics.width as f32) / self.width as f32,
            y1: (self.next_y as f32 + metrics.height as f32) / self.height as f32,
            xoff: metrics.xmin as f32,
            yoff: -(metrics.height as f32 + metrics.ymin as f32),
        };

        // XXX(xarkes): Not sure why I am doing +1... but it solves some artefacts showing up. Maybe there is a bug somewhere else?
        self.next_x += metrics.width + 1;
        self.cur_max_height = std::cmp::max(self.cur_max_height, metrics.height);

        glyph
    }
}

#[cfg(any(feature = "fontdue", all(feature = "freetype", target_os = "linux")))]
struct GlyphCache {
    table: LruCache<char, Glyph>,
    table_ascii: [Glyph; 256],
}

// ============================================================================
// Fontdue backend (default)
// ============================================================================

#[cfg(feature = "fontdue")]
pub struct FontCache {
    font: fontdue::Font,
    glyph_cache: HashMap<u32, GlyphCache>,
    atlas: Atlas,
    pub(crate) dirty: bool,
    pub(crate) texture_id: u32,
}

#[cfg(feature = "fontdue")]
impl FontCache {
    pub fn new(font_bytes: &[u8]) -> Self {
        let t0 = std::time::Instant::now();
        let font = fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default()).unwrap();
        println!(
            "[profile]   fontdue::Font::from_bytes: {:?} (input: {} KB)",
            t0.elapsed(),
            font_bytes.len() / 1024
        );

        let t1 = std::time::Instant::now();
        let atlas = Atlas::new();
        println!("[profile]   Atlas::new: {:?}", t1.elapsed());

        FontCache {
            font,
            glyph_cache: HashMap::new(),
            atlas,
            dirty: true,
            texture_id: 0,
        }
    }

    /// Add a glyph to the cache
    /// Must be called only if you are sure the glyph is not in the cache already
    fn add(&mut self, glyph: char, size: f32) -> Option<&Glyph> {
        let (keysize, quantized_size) = quantize_size(size);
        if !self.font.has_glyph(glyph) {
            return None;
        }

        let cache = self.glyph_cache.get_mut(&keysize).unwrap();
        debug_assert!(cache.table.peek(&glyph).is_none());
        let (metrics, bitmap) = self.font.rasterize(glyph, quantized_size);
        let raster_metrics = RasterMetrics {
            width: metrics.width,
            height: metrics.height,
            advance_width: metrics.advance_width,
            xmin: metrics.xmin,
            ymin: metrics.ymin,
        };
        let glyph_data = self.atlas.add_glyph(raster_metrics, bitmap);
        self.dirty = true;
        if glyph.len_utf8() == 1 {
            cache.table_ascii[glyph as u8 as usize] = glyph_data;
            Some(&cache.table_ascii[glyph as u8 as usize])
        } else {
            cache.table.put(glyph, glyph_data);
            Some(cache.table.peek(&glyph).unwrap())
        }
    }

    fn ensure_size_cache(&mut self, size: f32) -> u32 {
        let (keysize, _) = quantize_size(size);

        if !self.glyph_cache.contains_key(&keysize) {
            self.glyph_cache.insert(
                keysize,
                GlyphCache {
                    table: LruCache::new(CACHE_GLYPH_COUNT),
                    table_ascii: [Glyph::default(); 256],
                },
            );
            // Pre-rasterize ASCII
            for ccode in 0..=255u8 {
                self.add(ccode as char, size);
            }
        }

        keysize
    }

    pub fn get(&mut self, glyph: char, size: f32) -> &Glyph {
        let (_, quantized_size) = quantize_size(size);
        let keysize = self.ensure_size_cache(size);

        // Fast path: ASCII (always pre-cached)
        if glyph.len_utf8() == 1 {
            return &self.glyph_cache.get(&keysize).unwrap().table_ascii[glyph as u8 as usize];
        }

        // Check if non-ASCII glyph is already cached (peek doesn't update LRU order)
        let needs_rasterize = self
            .glyph_cache
            .get(&keysize)
            .unwrap()
            .table
            .peek(&glyph)
            .is_none();

        if needs_rasterize {
            // Rasterize and cache the glyph
            if self.font.has_glyph(glyph) {
                let (metrics, bitmap) = self.font.rasterize(glyph, quantized_size);
                let raster_metrics = RasterMetrics {
                    width: metrics.width,
                    height: metrics.height,
                    advance_width: metrics.advance_width,
                    xmin: metrics.xmin,
                    ymin: metrics.ymin,
                };
                let glyph_data = self.atlas.add_glyph(raster_metrics, bitmap);
                self.dirty = true;
                self.glyph_cache
                    .get_mut(&keysize)
                    .unwrap()
                    .table
                    .put(glyph, glyph_data);
            } else {
                // Fallback to '?'
                return &self.glyph_cache.get(&keysize).unwrap().table_ascii['?' as usize];
            }
        }

        // Use get to update LRU order, then return reference via peek
        self.glyph_cache
            .get_mut(&keysize)
            .unwrap()
            .table
            .get(&glyph);
        self.glyph_cache
            .get(&keysize)
            .unwrap()
            .table
            .peek(&glyph)
            .unwrap()
    }

    /// Retrieve the current atlas
    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    /// Returns the nearest valid cursor position given one.
    pub fn get_cursor_position(&mut self, size: f32, text: &str, cursorx: f32) -> (f32, usize) {
        let mut length = 0.;
        let mut idx = 0;
        for c in text.chars() {
            let glyph = self.get(c, size);
            if cursorx > length + glyph.advance / 2. {
                length += glyph.advance;
            } else {
                break;
            }
            idx += 1;
        }
        (length, idx)
    }

    pub fn get_text_size(&mut self, size: f32, text: &str, length: usize) -> (f32, f32) {
        let mut width = 0.;
        for (i, c) in text.char_indices() {
            if i >= length {
                break;
            }
            if c == '\t' {
                continue;
            }
            let glyph = self.get(c, size);
            width += glyph.advance;
        }
        (width, self.line_height(size))
    }

    pub fn line_height(&self, font_size: f32) -> f32 {
        self.font
            .horizontal_line_metrics(font_size)
            .expect("font size error")
            .new_line_size
    }
}

// ============================================================================
// FreeType backend
// ============================================================================

#[cfg(all(feature = "freetype", not(feature = "fontdue"), target_os = "linux"))]
use freetype::Library as FtLibrary;
#[cfg(all(feature = "freetype", not(feature = "fontdue"), target_os = "linux"))]
use freetype::face::LoadFlag;

#[cfg(all(feature = "freetype", not(feature = "fontdue"), target_os = "linux"))]
pub struct FontCache {
    library: FtLibrary,
    face_data: Vec<u8>, // Keep font data alive for the face
    face: freetype::Face,
    glyph_cache: HashMap<u32, GlyphCache>,
    atlas: Atlas,
    pub(crate) dirty: bool,
    pub(crate) texture_id: u32,
}

#[cfg(all(feature = "freetype", not(feature = "fontdue"), target_os = "linux"))]
impl FontCache {
    pub fn new(font_bytes: &[u8]) -> Self {
        let t0 = std::time::Instant::now();
        let library = FtLibrary::init().expect("Failed to initialize FreeType");

        // FreeType requires the font data to remain valid for the lifetime of the face,
        // so we keep a copy of it
        let face_data = font_bytes.to_vec();
        let face = library
            .new_memory_face(face_data.clone(), 0)
            .expect("Failed to load font face");

        println!(
            "[profile]   freetype::Face::new: {:?} (input: {} KB)",
            t0.elapsed(),
            font_bytes.len() / 1024
        );

        let t1 = std::time::Instant::now();
        let atlas = Atlas::new();
        println!("[profile]   Atlas::new: {:?}", t1.elapsed());

        FontCache {
            library,
            face_data,
            face,
            glyph_cache: HashMap::new(),
            atlas,
            dirty: true,
            texture_id: 0,
        }
    }

    /// Rasterize a glyph using FreeType
    fn rasterize(&self, glyph: char, size: f32) -> Option<(RasterMetrics, Vec<u8>)> {
        let pixel_size = size.round().max(1.0) as u32;
        self.face
            .set_pixel_sizes(0, pixel_size)
            .expect("Failed to set pixel size");

        // Get glyph index (returns Option<u32>)
        let glyph_index = self.face.get_char_index(glyph as usize)?;

        // Load the glyph
        self.face
            .load_glyph(glyph_index, LoadFlag::RENDER)
            .expect("Failed to load glyph");

        let glyph_slot = self.face.glyph();
        let bitmap = glyph_slot.bitmap();
        let metrics = glyph_slot.metrics();

        let width = bitmap.width() as usize;
        let height = bitmap.rows() as usize;

        // Copy bitmap data (FreeType uses top-down, we need to copy row by row)
        let mut data = vec![0u8; width * height];
        let buffer = bitmap.buffer();
        let pitch = bitmap.pitch().unsigned_abs() as usize;

        for y in 0..height {
            let src_offset = y * pitch;
            let dst_offset = y * width;
            if src_offset + width <= buffer.len() && dst_offset + width <= data.len() {
                data[dst_offset..dst_offset + width]
                    .copy_from_slice(&buffer[src_offset..src_offset + width]);
            }
        }

        // FreeType metrics are in 26.6 fixed-point format (divide by 64)
        let raster_metrics = RasterMetrics {
            width,
            height,
            advance_width: (metrics.horiAdvance >> 6) as f32,
            xmin: glyph_slot.bitmap_left(),
            ymin: glyph_slot.bitmap_top() - height as i32,
        };

        Some((raster_metrics, data))
    }

    /// Check if font has a glyph
    fn has_glyph(&self, glyph: char) -> bool {
        self.face.get_char_index(glyph as usize).is_some()
    }

    /// Add a glyph to the cache
    fn add(&mut self, glyph: char, size: f32) -> Option<&Glyph> {
        let (keysize, quantized_size) = quantize_size(size);

        // Rasterize first (before borrowing glyph_cache mutably)
        let (metrics, bitmap) = self.rasterize(glyph, quantized_size)?;

        let cache = self.glyph_cache.get_mut(&keysize).unwrap();
        debug_assert!(cache.table.peek(&glyph).is_none());

        let glyph_data = self.atlas.add_glyph(metrics, bitmap);
        self.dirty = true;

        if glyph.len_utf8() == 1 {
            cache.table_ascii[glyph as u8 as usize] = glyph_data;
            Some(&cache.table_ascii[glyph as u8 as usize])
        } else {
            cache.table.put(glyph, glyph_data);
            Some(cache.table.peek(&glyph).unwrap())
        }
    }

    fn ensure_size_cache(&mut self, size: f32) -> u32 {
        let (keysize, _) = quantize_size(size);

        if !self.glyph_cache.contains_key(&keysize) {
            self.glyph_cache.insert(
                keysize,
                GlyphCache {
                    table: LruCache::new(CACHE_GLYPH_COUNT),
                    table_ascii: [Glyph::default(); 256],
                },
            );
            // Pre-rasterize ASCII
            for ccode in 0..=255u8 {
                self.add(ccode as char, size);
            }
        }

        keysize
    }

    pub fn get(&mut self, glyph: char, size: f32) -> &Glyph {
        let (_, quantized_size) = quantize_size(size);
        let keysize = self.ensure_size_cache(size);

        // Fast path: ASCII (always pre-cached)
        if glyph.len_utf8() == 1 {
            return &self.glyph_cache.get(&keysize).unwrap().table_ascii[glyph as u8 as usize];
        }

        // Check if non-ASCII glyph is already cached
        let needs_rasterize = self
            .glyph_cache
            .get(&keysize)
            .unwrap()
            .table
            .peek(&glyph)
            .is_none();

        if needs_rasterize {
            if let Some((metrics, bitmap)) = self.rasterize(glyph, quantized_size) {
                let glyph_data = self.atlas.add_glyph(metrics, bitmap);
                self.dirty = true;
                self.glyph_cache
                    .get_mut(&keysize)
                    .unwrap()
                    .table
                    .put(glyph, glyph_data);
            } else {
                // Fallback to '?'
                return &self.glyph_cache.get(&keysize).unwrap().table_ascii['?' as usize];
            }
        }

        // Use get to update LRU order, then return reference via peek
        self.glyph_cache
            .get_mut(&keysize)
            .unwrap()
            .table
            .get(&glyph);
        self.glyph_cache
            .get(&keysize)
            .unwrap()
            .table
            .peek(&glyph)
            .unwrap()
    }

    /// Retrieve the current atlas
    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    /// Returns the nearest valid cursor position given one.
    pub fn get_cursor_position(&mut self, size: f32, text: &str, cursorx: f32) -> (f32, usize) {
        let mut length = 0.;
        let mut idx = 0;
        for c in text.chars() {
            let glyph = self.get(c, size);
            if cursorx > length + glyph.advance / 2. {
                length += glyph.advance;
            } else {
                break;
            }
            idx += 1;
        }
        (length, idx)
    }

    pub fn get_text_size(&mut self, size: f32, text: &str, length: usize) -> (f32, f32) {
        let mut width = 0.;
        for (i, c) in text.char_indices() {
            if i >= length {
                break;
            }
            if c == '\t' {
                continue;
            }
            let glyph = self.get(c, size);
            width += glyph.advance;
        }
        (width, self.line_height(size))
    }

    pub fn line_height(&self, font_size: f32) -> f32 {
        let pixel_size = font_size.round().max(1.0) as u32;
        self.face
            .set_pixel_sizes(0, pixel_size)
            .expect("Failed to set pixel size");

        // FreeType height is in 26.6 fixed-point format
        let height = self.face.size_metrics().map(|m| m.height >> 6).unwrap_or(0);
        height as f32
    }
}

// ============================================================================
// Fallback backend
// ============================================================================

#[cfg(not(any(feature = "fontdue", all(feature = "freetype", target_os = "linux"))))]
pub struct FontCache {
    glyph: Glyph,
    atlas: Atlas,
    pub(crate) dirty: bool,
    pub(crate) texture_id: u32,
}

#[cfg(not(any(feature = "fontdue", all(feature = "freetype", target_os = "linux"))))]
impl FontCache {
    pub fn new(_font_bytes: &[u8]) -> Self {
        FontCache {
            glyph: Glyph::default(),
            atlas: Atlas::new(),
            dirty: true,
            texture_id: 0,
        }
    }

    pub fn get(&mut self, _glyph: char, size: f32) -> &Glyph {
        self.glyph = Glyph {
            width: 0,
            height: 0,
            advance: size * 0.6,
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
            xoff: 0.0,
            yoff: 0.0,
        };
        &self.glyph
    }

    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    pub fn get_cursor_position(&mut self, size: f32, text: &str, cursorx: f32) -> (f32, usize) {
        let advance = size * 0.6;
        let idx = (cursorx / advance).round().max(0.0) as usize;
        let capped = idx.min(text.chars().count());
        (capped as f32 * advance, capped)
    }

    pub fn get_text_size(&mut self, size: f32, text: &str, length: usize) -> (f32, f32) {
        let count = text
            .char_indices()
            .take_while(|(idx, _)| *idx < length)
            .filter(|(_, c)| *c != '\t')
            .count();
        (count as f32 * size * 0.6, size)
    }

    pub fn line_height(&self, font_size: f32) -> f32 {
        font_size * 1.2
    }
}

// On Linux: both shouldn't be enabled simultaneously
