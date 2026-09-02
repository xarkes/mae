//! OS-native font fallback: given a codepoint the primary face cannot render,
//! ask the platform which installed font covers it and load its bytes so we can
//! shape + rasterize with it.
//!
//! Implemented for macOS (CoreText), Windows (DirectWrite `IDWriteFontFallback`),
//! and Linux (fontconfig) — the supported desktop targets.

/// A font file located by the OS, ready to be turned into a face. `index` is the
/// face index within a TrueType collection (`.ttc`); 0 for single-face files.
pub struct LoadedFont {
    /// Stable dedup key (the font file path) so repeated lookups share one face.
    pub key: String,
    pub index: u32,
    /// Font file bytes, but only when the locator already had to read them (macOS
    /// needs the bytes to pick the collection face index). Leaving this `None` lets
    /// the font cache skip the read whenever the face is already interned — critical
    /// on the typing hot path, where many distinct CJK codepoints all resolve to the
    /// same multi-MB system font and re-reading it per codepoint stalls input.
    pub bytes: Option<Vec<u8>>,
}

#[cfg(target_os = "macos")]
pub fn locate(c: char) -> Option<LoadedFont> {
    macos::locate_query(c, &c.to_string())
}

/// Like [`locate`] but requests *emoji* presentation by appending the emoji
/// variation selector (U+FE0F) to the query, so the OS returns a color emoji font
/// for codepoints that also have a monochrome text form (e.g. U+2764 ❤).
#[cfg(target_os = "macos")]
pub fn locate_emoji(c: char) -> Option<LoadedFont> {
    macos::locate_query(c, &format!("{c}\u{FE0F}"))
}

#[cfg(target_os = "windows")]
pub fn locate(c: char) -> Option<LoadedFont> {
    windows_impl::locate_query(c, false)
}

/// See [`locate`]; requests *emoji* presentation by appending U+FE0F to the query
/// so the OS returns a color emoji font for dual-presentation codepoints.
#[cfg(target_os = "windows")]
pub fn locate_emoji(c: char) -> Option<LoadedFont> {
    windows_impl::locate_query(c, true)
}

#[cfg(target_os = "linux")]
pub fn locate(c: char) -> Option<LoadedFont> {
    linux::locate_query(c, false)
}

/// See [`locate`]; requests a *color* font (fontconfig `FC_COLOR`) so emoji
/// codepoints resolve to a color emoji font (e.g. Noto Color Emoji).
#[cfg(target_os = "linux")]
pub fn locate_emoji(c: char) -> Option<LoadedFont> {
    linux::locate_query(c, true)
}

/// No OS font-fallback service exists in the browser (the DOM render path
/// never reaches this: text is real DOM/`<input>` content, shaped and
/// rasterized by the browser itself — see `imui/paint_dom.rs`). Only present
/// so the crate compiles for `target_arch = "wasm32"` at all.
#[cfg(target_arch = "wasm32")]
pub fn locate(_c: char) -> Option<LoadedFont> {
    None
}

#[cfg(target_arch = "wasm32")]
pub fn locate_emoji(_c: char) -> Option<LoadedFont> {
    None
}

/// Pick the face index within a (possibly collection) font file that covers `c`.
/// Returns 0 for single-face files or when nothing matches.
#[cfg(target_os = "macos")]
fn collection_index_for(bytes: &[u8], c: char) -> u32 {
    use read_fonts::{FileRef, TableProvider};

    let covers = |font: &read_fonts::FontRef| -> bool {
        font.cmap()
            .ok()
            .and_then(|cmap| cmap.map_codepoint(c as u32))
            .is_some()
    };

    match FileRef::new(bytes) {
        Ok(FileRef::Collection(collection)) => {
            for i in 0..collection.len() {
                if let Ok(font) = collection.get(i) {
                    if covers(&font) {
                        return i;
                    }
                }
            }
            0
        }
        _ => 0,
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{LoadedFont, collection_index_for};
    use objc2_core_foundation::{CFRange, CFString, CFURL, CFURLPathStyle};
    use objc2_core_text::{CTFont, kCTFontURLAttribute};

    /// Locate the font CoreText uses to render `query`, then load its bytes. The
    /// collection index is chosen by coverage of `coverage_char` (the base
    /// codepoint), independent of any trailing presentation selectors in `query`.
    pub fn locate_query(coverage_char: char, query: &str) -> Option<LoadedFont> {
        // A base font to seed the cascade. Any registered font works; CoreText
        // returns whichever installed font actually covers the string.
        let base_name = CFString::from_str("Helvetica");
        let base = unsafe { CTFont::with_name(&base_name, 12.0, std::ptr::null()) };

        let query = CFString::from_str(query);
        let range = CFRange::new(0, query.length());
        let matched = unsafe { base.for_string(&query, range) };

        let url_attr = unsafe { matched.attribute(kCTFontURLAttribute)? };
        let url = url_attr.downcast_ref::<CFURL>()?;
        let path = url.file_system_path(CFURLPathStyle::CFURLPOSIXPathStyle)?;
        let path = path.to_string();

        let bytes = std::fs::read(&path).ok()?;
        let index = collection_index_for(&bytes, coverage_char);
        Some(LoadedFont {
            key: path,
            index,
            bytes: Some(bytes),
        })
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::LoadedFont;
    use std::cell::RefCell;
    use windows::Win32::Graphics::DirectWrite::*;
    use windows::core::*;

    thread_local! {
        /// Cached system font-fallback object (and the factory keeping it alive).
        /// `locate` is already memoised per-codepoint upstream, but a run of CJK or
        /// emoji still produces many distinct codepoints, so we avoid rebuilding the
        /// DirectWrite factory on every miss.
        static FALLBACK: RefCell<Option<(IDWriteFactory2, IDWriteFontFallback)>> =
            const { RefCell::new(None) };
    }

    /// `IDWriteTextAnalysisSource` over a fixed UTF-16 slice. `MapCharacters` reads
    /// the text through this, so we hand it back the whole string at position 0 and
    /// nothing before it.
    #[implement(IDWriteTextAnalysisSource)]
    struct AnalysisSource {
        /// The query text (the codepoint, plus U+FE0F for emoji), UTF-16.
        text: Vec<u16>,
        /// Null-terminated locale name returned for the whole run.
        locale: Vec<u16>,
    }

    impl IDWriteTextAnalysisSource_Impl for AnalysisSource_Impl {
        fn GetTextAtPosition(
            &self,
            text_position: u32,
            text_string: *mut *mut u16,
            text_length: *mut u32,
        ) -> Result<()> {
            unsafe {
                let pos = text_position as usize;
                if pos >= self.text.len() {
                    *text_string = std::ptr::null_mut();
                    *text_length = 0;
                } else {
                    *text_string = self.text.as_ptr().add(pos) as *mut u16;
                    *text_length = (self.text.len() - pos) as u32;
                }
            }
            Ok(())
        }

        fn GetTextBeforePosition(
            &self,
            text_position: u32,
            text_string: *mut *mut u16,
            text_length: *mut u32,
        ) -> Result<()> {
            unsafe {
                let pos = text_position as usize;
                if pos == 0 || pos > self.text.len() {
                    *text_string = std::ptr::null_mut();
                    *text_length = 0;
                } else {
                    *text_string = self.text.as_ptr() as *mut u16;
                    *text_length = pos as u32;
                }
            }
            Ok(())
        }

        fn GetParagraphReadingDirection(&self) -> DWRITE_READING_DIRECTION {
            DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
        }

        fn GetLocaleName(
            &self,
            text_position: u32,
            text_length: *mut u32,
            locale_name: *mut *mut u16,
        ) -> Result<()> {
            unsafe {
                *text_length = (self.text.len() as u32).saturating_sub(text_position);
                *locale_name = self.locale.as_ptr() as *mut u16;
            }
            Ok(())
        }

        fn GetNumberSubstitution(
            &self,
            text_position: u32,
            text_length: *mut u32,
            _number_substitution: OutRef<IDWriteNumberSubstitution>,
        ) -> Result<()> {
            unsafe {
                *text_length = (self.text.len() as u32).saturating_sub(text_position);
            }
            Ok(())
        }
    }

    /// Ask DirectWrite which installed font renders `c` (with emoji presentation when
    /// `emoji`), then resolve that font's on-disk file + face index and read its bytes.
    pub fn locate_query(c: char, emoji: bool) -> Option<LoadedFont> {
        let mut text: Vec<u16> = Vec::with_capacity(3);
        let mut buf = [0u16; 2];
        text.extend_from_slice(c.encode_utf16(&mut buf));
        if emoji {
            text.push(0xFE0F);
        }
        let text_len = text.len() as u32;

        let source: IDWriteTextAnalysisSource = AnalysisSource {
            text,
            // UTF-16, null-terminated. A generic locale is fine: fallback is driven
            // by codepoint coverage, not language.
            locale: "en-us\0".encode_utf16().collect(),
        }
        .into();

        FALLBACK.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                // SAFETY: DWriteCreateFactory returns a factory we own; the shared
                // factory needs no COM apartment init.
                let factory: IDWriteFactory2 =
                    unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()? };
                let fallback = unsafe { factory.GetSystemFontFallback().ok()? };
                *slot = Some((factory, fallback));
            }
            let (_factory, fallback) = slot.as_ref()?;

            let mut mapped_len: u32 = 0;
            let mut mapped_font: Option<IDWriteFont> = None;
            let mut scale: f32 = 0.0;
            // SAFETY: all out-params are valid; the source outlives the call.
            unsafe {
                fallback
                    .MapCharacters(
                        &source,
                        0,
                        text_len,
                        None,
                        w!("Segoe UI"),
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_FONT_STYLE_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        &mut mapped_len,
                        &mut mapped_font,
                        &mut scale,
                    )
                    .ok()?;
            }
            let font = mapped_font?;
            font_to_loaded(&font)
        })
    }

    /// Resolve an `IDWriteFont` to its backing file path + face index, then read it.
    fn font_to_loaded(font: &IDWriteFont) -> Option<LoadedFont> {
        unsafe {
            let face = font.CreateFontFace().ok()?;
            let index = face.GetIndex();

            // Fetch the single font file backing this face.
            let mut file_count: u32 = 0;
            face.GetFiles(&mut file_count, None).ok()?;
            if file_count == 0 {
                return None;
            }
            let mut files: Vec<Option<IDWriteFontFile>> = vec![None; 1];
            face.GetFiles(&mut 1, Some(files.as_mut_ptr())).ok()?;
            let file = files.into_iter().next().flatten()?;

            // The reference key + local loader yield the on-disk path.
            let mut key_ptr: *mut core::ffi::c_void = std::ptr::null_mut();
            let mut key_size: u32 = 0;
            file.GetReferenceKey(&mut key_ptr, &mut key_size).ok()?;
            let loader = file.GetLoader().ok()?;
            let local: IDWriteLocalFontFileLoader = loader.cast().ok()?;

            let path_len = local.GetFilePathLengthFromKey(key_ptr, key_size).ok()?;
            let mut path_buf = vec![0u16; path_len as usize + 1];
            local
                .GetFilePathFromKey(key_ptr, key_size, &mut path_buf)
                .ok()?;
            let path = String::from_utf16_lossy(&path_buf[..path_len as usize]);

            // `GetIndex` gives the exact face index, so we never need to read the
            // file just to locate it. Defer the (potentially multi-MB) read to the
            // font cache, which only reads on a genuine new-face miss.
            Some(LoadedFont {
                key: path,
                index,
                bytes: None,
            })
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::LoadedFont;
    use std::ffi::{CStr, c_char, c_int, c_void};
    use std::sync::Once;

    // fontconfig opaque handles and primitive aliases.
    type FcChar8 = u8;
    type FcChar32 = u32;
    type FcBool = c_int;
    type FcConfig = c_void;
    type FcPattern = c_void;
    type FcCharSet = c_void;

    // FcMatchKind::FcMatchPattern and FcResult::FcResultMatch.
    const FC_MATCH_PATTERN: c_int = 0;
    const FC_RESULT_MATCH: c_int = 0;

    #[link(name = "fontconfig")]
    unsafe extern "C" {
        fn FcInit() -> FcBool;
        fn FcPatternCreate() -> *mut FcPattern;
        fn FcPatternDestroy(p: *mut FcPattern);
        fn FcCharSetCreate() -> *mut FcCharSet;
        fn FcCharSetDestroy(cs: *mut FcCharSet);
        fn FcCharSetAddChar(cs: *mut FcCharSet, ucs4: FcChar32) -> FcBool;
        fn FcPatternAddCharSet(
            p: *mut FcPattern,
            object: *const c_char,
            cs: *const FcCharSet,
        ) -> FcBool;
        fn FcPatternAddBool(p: *mut FcPattern, object: *const c_char, b: FcBool) -> FcBool;
        fn FcConfigSubstitute(config: *mut FcConfig, p: *mut FcPattern, kind: c_int) -> FcBool;
        fn FcDefaultSubstitute(p: *mut FcPattern);
        fn FcFontMatch(
            config: *mut FcConfig,
            p: *mut FcPattern,
            result: *mut c_int,
        ) -> *mut FcPattern;
        fn FcPatternGetString(
            p: *const FcPattern,
            object: *const c_char,
            n: c_int,
            s: *mut *mut FcChar8,
        ) -> c_int;
        fn FcPatternGetInteger(
            p: *const FcPattern,
            object: *const c_char,
            n: c_int,
            i: *mut c_int,
        ) -> c_int;
    }

    /// Ask fontconfig which installed font covers `c`, requesting a color font when
    /// `emoji`. Build a pattern whose charset holds the single codepoint, run the
    /// usual config + default substitutions, and match. Returns the matched font's
    /// path + face index; the bytes are deferred to the font cache (it dedups by
    /// path+index first, so an already-loaded fallback font is never re-read).
    pub fn locate_query(c: char, emoji: bool) -> Option<LoadedFont> {
        static INIT: Once = Once::new();
        INIT.call_once(|| unsafe {
            FcInit();
        });

        unsafe {
            let pat = FcPatternCreate();
            if pat.is_null() {
                return None;
            }
            let cs = FcCharSetCreate();
            if cs.is_null() {
                FcPatternDestroy(pat);
                return None;
            }
            FcCharSetAddChar(cs, c as FcChar32);
            // FcPatternAddCharSet copies the charset, so our reference is freed below.
            FcPatternAddCharSet(pat, c"charset".as_ptr(), cs);
            if emoji {
                FcPatternAddBool(pat, c"color".as_ptr(), 1);
            }

            FcConfigSubstitute(std::ptr::null_mut(), pat, FC_MATCH_PATTERN);
            FcDefaultSubstitute(pat);

            let mut result: c_int = 0;
            let matched = FcFontMatch(std::ptr::null_mut(), pat, &mut result);
            FcCharSetDestroy(cs);
            FcPatternDestroy(pat);

            if matched.is_null() || result != FC_RESULT_MATCH {
                if !matched.is_null() {
                    FcPatternDestroy(matched);
                }
                return None;
            }

            // The returned string points into the matched pattern; copy it out
            // before destroying the pattern.
            let mut file_ptr: *mut FcChar8 = std::ptr::null_mut();
            let path = if FcPatternGetString(matched, c"file".as_ptr(), 0, &mut file_ptr)
                == FC_RESULT_MATCH
                && !file_ptr.is_null()
            {
                CStr::from_ptr(file_ptr as *const c_char)
                    .to_string_lossy()
                    .into_owned()
            } else {
                FcPatternDestroy(matched);
                return None;
            };

            let mut index: c_int = 0;
            FcPatternGetInteger(matched, c"index".as_ptr(), 0, &mut index);

            FcPatternDestroy(matched);

            Some(LoadedFont {
                key: path,
                index: index.max(0) as u32,
                bytes: None,
            })
        }
    }
}
