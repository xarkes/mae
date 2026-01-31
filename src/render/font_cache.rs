// TODO(perf): Font parsing is slow at startup (~160ms for NotoSans, ~83ms for MaterialIcons).
// TODO(memory): fontdue uses significant RAM for parsed font structures (~50-100MB for large fonts).
//
// Consider using native font APIs for better performance AND memory usage:
//   - Linux: FreeType (`freetype-rs` crate) - system library, highly optimized
//   - macOS: Core Text (`core-text` crate) - hardware accelerated
//   - Windows: DirectWrite (`dwrote` crate) - hardware accelerated
// Native APIs benefit from:
//   1. Optimized C/C++ font parsing
//   2. System-level glyph caching (shared across apps)
//   3. Font files often already memory-mapped by OS
//   4. Better platform-specific hinting
//   5. Lower memory footprint (shared system font data)

use std::collections::HashMap;
use std::num::NonZeroUsize;

use lru::LruCache;

const CACHE_GLYPH_COUNT: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(512) };

/// Quantize font size to avoid floating point precision issues.
/// Rounds to nearest 0.5pt, returns a key suitable for HashMap lookup.
#[inline]
fn quantize_size(size: f32) -> (u32, f32) {
    // Round to nearest 0.5 (multiply by 2, round, divide by 2)
    let quantized = (size * 2.0).round() / 2.0;
    let key = (quantized * 2.0) as u32; // Unique key for each 0.5 increment
    (key, quantized)
}
const ATLAS_WIDTH: usize = 1024;

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
    pub fn add_glyph(&mut self, metrics: fontdue::Metrics, bitmap: Vec<u8>) -> Glyph {
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

struct GlyphCache {
    table: LruCache<char, Glyph>,
    table_ascii: [Glyph; 256],
}

pub struct FontCache {
    font: fontdue::Font,
    glyph_cache: HashMap<u32, GlyphCache>,
    atlas: Atlas,
    pub(crate) dirty: bool,
    pub(crate) texture_id: u32,
}
impl FontCache {
    pub fn new(font_bytes: &[u8]) -> Self {
        let t0 = std::time::Instant::now();
        let font = fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default()).unwrap();
        println!("[profile]   fontdue::Font::from_bytes: {:?} (input: {} KB)",
                 t0.elapsed(), font_bytes.len() / 1024);

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
        // TODO(xarkes): Usually we will call draw_text later on, so we can avoid useless heavy calls by caching what was done in this function
        let mut width = 0.;
        let mut height = 0;
        for (i, c) in text.char_indices() {
            if i >= length {
                break;
            }
            if c == '\t' {
                continue;
            }
            let glyph = self.get(c, size);
            width += glyph.advance;
            height = std::cmp::max(height, glyph.height);
        }
        (width, height as f32)
    }

    pub fn line_height(&self, font_size: f32) -> f32 {
        self.font
            .horizontal_line_metrics(font_size)
            .expect("font size error")
            .new_line_size
    }
}
