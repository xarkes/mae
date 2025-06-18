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

#[derive(Clone)]
pub struct Glyph {
    pub atlas_idx: usize,
    pub size: usize,
}

pub struct FontCache {
    font: fontdue::Font,
    table: LRUCache<char, Glyph>,
    atlas: Vec<u8>,
}

impl FontCache {
    pub fn new() -> Self {
        // XXX(xarkes): It seems that fontdue is not able to handle emojis rasterization.
        // In addition, we may want in the future to have a way to handle font "fallback"
        // i.e. looking up for a glyph in a separate font when the current one does not provide it.
        let font = include_bytes!("/System/Library/Fonts/SFNSMono.ttf") as &[u8];
        // let font = include_bytes!("/System/Library/Fonts/SFNS.ttf") as &[u8];
        // let font =
        //     include_bytes!("/Users/user/Downloads/Noto_Color_Emoji/NotoColorEmoji-Regular.ttf")
        //         as &[u8];
        // let font = include_bytes!("/System/Library/Fonts/Apple Symbols.ttf") as &[u8];
        let font = fontdue::Font::from_bytes(font, fontdue::FontSettings::default()).unwrap();
        const CACHE_GLYPH_COUNT: usize = 512;
        // let w = 128;
        // let h = 128;
        let mut fc = FontCache {
            font,
            table: LRUCache::new(CACHE_GLYPH_COUNT),
            // atlas: Vec::with_capacity(CACHE_GLYPH_COUNT * w * h),
            atlas: Vec::new(),
        };

        // for ccode in 33..127u8 {
        //     fc.add(ccode as char);
        // }
        fc.add('A');
        fc
    }

    /// Add a glyph to the cache
    fn add(&mut self, glyph: char) -> Option<Glyph> {
        if !self.font.has_glyph(glyph) {
            println!(
                "glyph '{:?}' not found, switch font?",
                glyph.to_string().into_bytes()
            );
            return None;
        }
        let (metrics, bitmap) = self.font.rasterize(glyph, 128.0);
        println!("Metrics: {:?} ({})", metrics, glyph);
        let idx = self.atlas.len();
        // TODO(xarkes): Support eviction on the atlas itself as well as the table
        self.atlas.extend(bitmap);
        let glyph_data = Glyph {
            atlas_idx: idx,
            size: metrics.width * metrics.height,
        };

        self.table.set(glyph, glyph_data.clone());

        Some(glyph_data)
    }

    /// Retrieve rasterized glyph
    /// If not in cache, add it
    pub fn get(&mut self, glyph: char) -> Option<Glyph> {
        let maybe_glyph = &mut self.table.get(&glyph);
        if !maybe_glyph.is_some() {
            self.add(glyph)
        } else {
            maybe_glyph.cloned()
        }
    }

    pub fn texture(&self) -> Vec<u8> {
        self.atlas.clone()
    }
}
