use std::collections::HashMap;
use std::hash::Hash;

struct LRUEntry<K, V> {
    key: K,
    val: V,
    prev: *mut LRUEntry<K, V>,
    next: *mut LRUEntry<K, V>,
}

struct LRUCache<K, V> {
    max: usize,
    first: *mut LRUEntry<K, V>,
    // last: *mut LRUEntry<K, V>,
    map: HashMap<K, Box<LRUEntry<K, V>>>,
}

impl<K, V> LRUCache<K, V>
where
    K: Hash + Eq + Clone,
{
    pub fn new(max: usize) -> Self {
        LRUCache {
            max,
            first: std::ptr::null_mut(),
            // last: std::ptr::null_mut(),
            map: HashMap::<K, Box<LRUEntry<K, V>>>::new(),
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        match self.map.get(key) {
            Some(entry) => Some(&entry.val),
            _ => None,
        }
    }

    pub fn set(&mut self, key: K, value: V) {
        match self.map.get_mut(&key) {
            Some(existing) => {
                // Element is in cache, move it to head
                // SAFETY: Pointer set from a Box pointer, which will (should?) never be moved
                if existing.prev != std::ptr::null_mut() {
                    unsafe { (*existing.prev).next = existing.next };
                }
                if existing.next != std::ptr::null_mut() {
                    unsafe { (*existing.next).prev = existing.prev };
                }
                existing.prev = self.first;
                existing.next = std::ptr::null_mut();
                self.first = existing.as_mut();
            }
            None => {
                // Element is not in cache, add it to head
                let mut entry = Box::new(LRUEntry {
                    key: key.clone(),
                    val: value,
                    prev: self.first,
                    next: std::ptr::null_mut(),
                });
                let entry_ptr: *mut LRUEntry<K, V> = &mut *entry;
                self.map.insert(key, entry);
                if self.first != std::ptr::null_mut() {
                    // SAFETY: Pointer set from a Box pointer, which will (should?) never be moved
                    unsafe { self.first.as_mut().unwrap().next = entry_ptr };
                }
                self.first = entry_ptr;
                // TODO(xarkes): Evict when we get above the max
            }
        }
    }
}

// TODO(xarkes): Seems like max glyph count is not really relevant, as what matters is if the atlas texture(s)
// is full or not
const CACHE_GLYPH_COUNT: usize = 512;
// TODO(xarkes): Rasterize for the right size?
const FONT_SIZE_RASTER: usize = 12;
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
    pub fn add_glyph(
        &mut self,
        metrics: fontdue::Metrics,
        line_metrics: fontdue::LineMetrics,
        bitmap: Vec<u8>,
    ) -> Glyph {
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
            // yoff: line_metrics.new_line_size - metrics.height as f32 - metrics.ymin as f32,
            yoff: -(metrics.height as f32 + metrics.ymin as f32),
        };

        // XXX(xarkes): Not sure why I am doing +1... but it solves some artefacts showing up. Maybe there is a bug somewhere else?
        self.next_x += metrics.width + 1;
        self.cur_max_height = std::cmp::max(self.cur_max_height, metrics.height);

        glyph
    }
}

pub struct FontCache {
    font: fontdue::Font,
    table: LRUCache<char, Glyph>,
    table_ascii: [Glyph; 256],
    atlas: Atlas,
}
impl FontCache {
    pub fn new() -> Self {
        // XXX(xarkes): It seems that fontdue is not able to handle emojis rasterization.
        // In addition, we may want in the future to have a way to handle font "fallback"
        // i.e. looking up for a glyph in a separate font when the current one does not provide it.
        // NOTE(xarkes): A quick search shows that apparently no font bundles all languages, so most likely we should
        // have multiple fonts (e.g. Google Noto) and load them depending on the language?
        // Not sure what's the best way to proceed here.
        // let font = include_bytes!("/System/Library/Fonts/SFNSMono.ttf") as &[u8];
        // let font = include_bytes!("/System/Library/Fonts/SFNS.ttf") as &[u8];
        let font = include_bytes!("/System/Library/Fonts/Menlo.ttc") as &[u8];
        // let font = include_bytes!("/tmp/fonts/Inconsolata-Regular.ttf") as &[u8];
        // let font =
        //     include_bytes!("/Users/user/Downloads/Noto_Color_Emoji/NotoColorEmoji-Regular.ttf")
        //         as &[u8];
        // let font = include_bytes!("/System/Library/Fonts/Apple Symbols.ttf") as &[u8];
        let font = fontdue::Font::from_bytes(font, fontdue::FontSettings::default()).unwrap();
        let mut fc = FontCache {
            font,
            table: LRUCache::new(CACHE_GLYPH_COUNT),
            table_ascii: [Glyph::default(); 256],
            atlas: Atlas::new(),
        };

        // xarkes: Pre-fill the cache for ASCII
        for ccode in 0..=255u8 {
            fc.add(ccode as char);
        }
        fc
    }

    /// Add a glyph to the cache
    /// Must be called only if you are sure the glyph is not in the cache already
    fn add(&mut self, glyph: char) -> Option<&Glyph> {
        assert!(self.table.get(&glyph).is_none());
        if !self.font.has_glyph(glyph) {
            // println!(
            //     "glyph '{:?}' not found, switch font?",
            //     glyph.to_string().into_bytes()
            // );
            return None;
        }
        let (metrics, bitmap) = self.font.rasterize(glyph, FONT_SIZE_RASTER as f32);
        let glyph_data = self.atlas.add_glyph(
            metrics,
            self.font
                .horizontal_line_metrics(FONT_SIZE_RASTER as f32)
                .unwrap(),
            bitmap,
        );
        if glyph.len_utf8() == 1 {
            self.table_ascii[glyph as u8 as usize] = glyph_data;
            Some(&self.table_ascii[glyph as u8 as usize])
        } else {
            self.table.set(glyph, glyph_data);
            self.table.get(&glyph)
        }
    }

    /// Retrieve rasterized glyph
    /// If not in cache, add it
    pub fn get(&mut self, glyph: char) -> (Option<&Glyph>, bool) {
        let mut added = false;
        // xarkes: For perf purpose, we already cached the ASCII table, just return the bitmap from it
        if glyph.len_utf8() == 1 {
            return (Some(&self.table_ascii[glyph as u8 as usize]), false);
        }
        // TODO(xarkes): For perf, make the hasmap use an optimized hash function (I suspect the current one to be too slow for this task), or have your own hashmap
        if self.table.get(&glyph).is_none() {
            self.add(glyph);
            added = true;
        }
        (self.table.get(&glyph), added)
    }

    /// Retrieve the current atlas
    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }
}
