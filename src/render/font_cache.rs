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

const CACHE_GLYPH_COUNT: usize = 512;
const FONT_SIZE_RASTER: usize = 128;
const ATLAS_WIDTH: usize = 2048;

#[derive(Clone, Debug)]
pub struct Glyph {
    pub tl_x: f32,
    pub tl_y: f32,
    pub br_x: f32,
    pub br_y: f32,
    pub yoff: f32,
    pub xoff: f32,
}

pub struct Atlas {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
    next_x: usize,
    next_y: usize,
}

impl Atlas {
    pub fn new() -> Self {
        Atlas {
            data: vec![0; ATLAS_WIDTH * ATLAS_WIDTH],
            width: ATLAS_WIDTH,
            height: ATLAS_WIDTH,
            next_x: 0,
            next_y: 0,
        }
    }

    /// Add a glyph to the current atlas
    pub fn add_glyph(&mut self, metrics: fontdue::Metrics, bitmap: Vec<u8>) -> Glyph {
        if self.next_y >= self.height {
            panic!("Full atlas is not handled yet");
        }
        assert!(metrics.width <= FONT_SIZE_RASTER);
        assert!(metrics.bounds.height <= FONT_SIZE_RASTER as f32);

        // Copy the square rasterized glyph in our atlas (non contiguous)
        for y in 0..metrics.height {
            let dst = &mut self.data[self.next_x + y * self.width + self.next_y * self.width
                ..self.next_x + y * self.width + self.next_y * self.width + metrics.width];
            let data = &bitmap[y * metrics.width..y * metrics.width + metrics.width];
            dst.copy_from_slice(data);
        }

        let glyph = Glyph {
            tl_x: (self.next_x as f32) / self.width as f32,
            tl_y: (self.next_y as f32) / self.height as f32,
            // NOTE(xarkes): -1 because there are FONT_SIZE_RASTER lines/cols, starting at 0. Without -1, we would include the next glyph pixels.
            br_x: (self.next_x as f32 + FONT_SIZE_RASTER as f32 - 1.0) / self.width as f32,
            br_y: (self.next_y as f32 + FONT_SIZE_RASTER as f32 - 1.0) / self.height as f32,
            yoff: (-metrics.bounds.height - metrics.bounds.ymin + FONT_SIZE_RASTER as f32)
                / FONT_SIZE_RASTER as f32,
            xoff: metrics.advance_width as f32 / FONT_SIZE_RASTER as f32,
        };
        self.next_x += FONT_SIZE_RASTER;
        if self.next_x >= self.width {
            self.next_x = 0;
            self.next_y += FONT_SIZE_RASTER;
        }
        glyph
    }
}

pub struct FontCache {
    font: fontdue::Font,
    table: LRUCache<char, Glyph>,
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
        let font = include_bytes!("/System/Library/Fonts/SFNS.ttf") as &[u8];
        // let font =
        //     include_bytes!("/Users/user/Downloads/Noto_Color_Emoji/NotoColorEmoji-Regular.ttf")
        //         as &[u8];
        // let font = include_bytes!("/System/Library/Fonts/Apple Symbols.ttf") as &[u8];
        let font = fontdue::Font::from_bytes(font, fontdue::FontSettings::default()).unwrap();
        // let w = 128;
        // let h = 128;
        let mut fc = FontCache {
            font,
            table: LRUCache::new(CACHE_GLYPH_COUNT),
            atlas: Atlas::new(),
        };

        for ccode in 33..127u8 {
            fc.add(ccode as char);
        }
        fc
    }

    /// Add a glyph to the cache
    /// Must be called only if you are sure the glyph is not in the cache already
    fn add(&mut self, glyph: char) -> Option<&Glyph> {
        assert!(self.table.get(&glyph).is_none());
        if !self.font.has_glyph(glyph) {
            println!(
                "glyph '{:?}' not found, switch font?",
                glyph.to_string().into_bytes()
            );
            return None;
        }
        let (metrics, bitmap) = self.font.rasterize(glyph, FONT_SIZE_RASTER as f32);
        let glyph_data = self.atlas.add_glyph(metrics, bitmap);
        self.table.set(glyph, glyph_data);
        self.table.get(&glyph)
    }

    /// Retrieve rasterized glyph
    /// If not in cache, add it
    pub fn get(&mut self, glyph: char) -> (Option<&Glyph>, bool) {
        let mut added = false;
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
