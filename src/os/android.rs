use super::OSEvent;
use android_activity::{
    AndroidApp, InputStatus, MainEvent, PollEvent,
    input::{InputEvent, KeyAction, KeyEvent, KeyMapChar, MotionAction},
};
use log::info;
pub struct Window {
    pub app: AndroidApp,
}

/// Tries to map the `key_event` to a `KeyMapChar` containing a unicode character or dead key accent
///
/// This shows how to take a `KeyEvent` and look up its corresponding `KeyCharacterMap` and
/// use that to try and map the `key_code` + `meta_state` to a unicode character or a
/// dead key that be combined with the next key press.
fn character_map_and_combine_key(
    app: &AndroidApp,
    key_event: &KeyEvent,
    combining_accent: &mut Option<char>,
) -> Option<KeyMapChar> {
    let device_id = key_event.device_id();

    let key_map = match app.device_key_character_map(device_id) {
        Ok(key_map) => key_map,
        Err(err) => {
            log::error!("Failed to look up `KeyCharacterMap` for device {device_id}: {err:?}");
            return None;
        }
    };

    match key_map.get(key_event.key_code(), key_event.meta_state()) {
        Ok(KeyMapChar::Unicode(unicode)) => {
            // Only do dead key combining on key down
            if key_event.action() == KeyAction::Down {
                let combined_unicode = if let Some(accent) = combining_accent {
                    match key_map.get_dead_char(*accent, unicode) {
                        Ok(Some(key)) => {
                            info!(
                                "KeyEvent: Combined '{unicode}' with accent '{accent}' to give '{key}'"
                            );
                            Some(key)
                        }
                        Ok(None) => None,
                        Err(err) => {
                            log::error!(
                                "KeyEvent: Failed to combine 'dead key' accent '{accent}' with '{unicode}': {err:?}"
                            );
                            None
                        }
                    }
                } else {
                    info!("KeyEvent: Pressed '{unicode}'");
                    Some(unicode)
                };
                *combining_accent = None;
                combined_unicode.map(|unicode| KeyMapChar::Unicode(unicode))
            } else {
                Some(KeyMapChar::Unicode(unicode))
            }
        }
        Ok(KeyMapChar::CombiningAccent(accent)) => {
            if key_event.action() == KeyAction::Down {
                info!("KeyEvent: Pressed 'dead key' combining accent '{accent}'");
                *combining_accent = Some(accent);
            }
            Some(KeyMapChar::CombiningAccent(accent))
        }
        Ok(KeyMapChar::None) => {
            // Leave any combining_accent state in tact (seems to match how other
            // Android apps work)
            info!("KeyEvent: Pressed non-unicode key");
            None
        }
        Err(err) => {
            log::error!("KeyEvent: Failed to get key map character: {err:?}");
            *combining_accent = None;
            None
        }
    }
}

impl Window {
    // IME (preedit/candidate positioning) is implemented on macOS only for now.
    pub fn ime_preedit(&self) -> Option<String> {
        None
    }

    pub fn set_ime_caret_rect(&self, _x: f32, _y: f32, _width: f32, _height: f32) {}

    pub fn new(app: AndroidApp) -> Self {
        Window { app }
    }
    pub fn wait_for_native_window(&self) {
        let mut native_window = None;
        while native_window.is_none() {
            self.app.poll_events(
                Some(std::time::Duration::from_secs(1)),
                |event| match event {
                    PollEvent::Main(main_event) => match main_event {
                        MainEvent::InitWindow { .. } => {
                            native_window = self.app.native_window();
                        }
                        _ => {}
                    },
                    _ => {}
                },
            );
        }
    }
    pub fn get_size(&self) -> (f32, f32) {
        // XXX: We have to find a better API to support Android
        (0., 0.)
    }
    pub fn refresh_rate_hz(&self) -> f32 {
        60.0
    }
    pub fn get_events(&self) -> Vec<OSEvent> {
        let events = Vec::new();

        let mut native_window = self.app.native_window();
        let mut combining_accent = None;
        let mut redraw_pending = false;

        self.app.poll_events(
            Some(std::time::Duration::from_secs(0)), /* timeout */
            // None,
            |event| {
                match event {
                    PollEvent::Wake => {
                        info!("Early wake up");
                    }
                    PollEvent::Timeout => {
                        // Real app would probably rely on vblank sync via graphics API...
                        redraw_pending = true;
                    }
                    PollEvent::Main(main_event) => {
                        info!("Main event: {:?}", main_event);
                        match main_event {
                            MainEvent::SaveState { saver, .. } => {
                                saver.store("foo://bar".as_bytes());
                            }
                            MainEvent::Pause => {}
                            MainEvent::Resume { loader, .. } => {
                                if let Some(state) = loader.load() {
                                    if let Ok(uri) = String::from_utf8(state) {
                                        info!("Resumed with saved state = {uri:#?}");
                                    }
                                }
                            }
                            MainEvent::InitWindow { .. } => {
                                // XXX: This event is not handled here
                                // ui = Some(IMUI::mobile(mae::os::Window { app: app.clone() }));
                                info!("InitWindow!");
                                redraw_pending = true;
                            }
                            MainEvent::TerminateWindow { .. } => {
                                native_window = None;
                            }
                            MainEvent::WindowResized { .. } => {
                                redraw_pending = true;
                            }
                            MainEvent::RedrawNeeded { .. } => {
                                redraw_pending = true;
                            }
                            MainEvent::InputAvailable { .. } => {
                                redraw_pending = true;
                            }
                            MainEvent::ConfigChanged { .. } => {
                                info!("Config Changed: {:#?}", self.app.config());
                            }
                            MainEvent::LowMemory => {}

                            MainEvent::Destroy => {}
                            _ => { /* ... */ }
                        }
                    }
                    _ => {}
                }

                if let Some(native_window) = &native_window {
                    redraw_pending = false;

                    // Handle input, via a lending iterator
                    match self.app.input_events_iter() {
                        Ok(mut iter) => loop {
                            if !iter.next(|event| {
                                match event {
                                    InputEvent::KeyEvent(key_event) => {
                                        let combined_key_char = character_map_and_combine_key(
                                            &self.app,
                                            key_event,
                                            &mut combining_accent,
                                        );
                                        info!("KeyEvent: combined key: {combined_key_char:?}")
                                    }
                                    InputEvent::MotionEvent(motion_event) => {
                                        println!("action = {:?}", motion_event.action());
                                        match motion_event.action() {
                                            MotionAction::Up => {
                                                let pointer = motion_event.pointer_index();
                                                let pointer =
                                                    motion_event.pointer_at_index(pointer);
                                                let x = pointer.x();
                                                let y = pointer.y();

                                                println!("POINTER UP {x}, {y}");
                                                if x < 200.0 && y < 200.0 {
                                                    println!("Requesting to show keyboard");
                                                    self.app.show_soft_input(true);
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    InputEvent::TextEvent(state) => {
                                        info!("Input Method State: {state:?}");
                                    }
                                    _ => {}
                                }

                                info!("Input Event: {event:?}");
                                InputStatus::Unhandled
                            }) {
                                break;
                            }
                        },
                        Err(err) => {
                            log::error!("Failed to get input events iterator: {err:?}");
                        }
                    }
                }
            },
        );

        events
    }
}
pub fn timer_init() -> f64 {
    1.
}
pub fn timer_value() -> u64 {
    1
}

/// No Android clipboard integration yet; the in-app clipboard fallback keeps copy/paste
/// working within the process.
pub fn clipboard_set(_text: &str) {}

/// No Android clipboard integration yet; see [`clipboard_set`].
pub fn clipboard_get() -> Option<String> {
    None
}

/// Image clipboard read - not yet implemented on this platform.
pub fn clipboard_get_image() -> Option<Vec<u8>> {
    None
}

/// Native image file picker - not yet implemented on this platform.
pub fn open_image_file_dialog() -> Option<std::path::PathBuf> {
    None
}
