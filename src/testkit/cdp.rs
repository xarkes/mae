//! [`UiDriver`] over the Chrome DevTools Protocol, driving a real page in a
//! locally installed headless Chromium — the browser-backed counterpart to
//! [`super::NativeDriver`]. A scenario written once against `impl UiDriver`
//! runs against either, so scenario code is never duplicated per backend.
//!
//! Talks to Chrome directly over its raw JSON-RPC-over-WebSocket protocol
//! (via `tungstenite`, blocking) rather than a full CDP client crate —
//! `mae` only ever needs `Runtime.evaluate`/`Input.dispatchMouseEvent`/
//! `Input.dispatchKeyEvent`/`Page.navigate`, a handful of calls, so a
//! minimal client keeps this test-only dependency footprint small and the
//! whole driver synchronous (no async runtime needed to match
//! [`super::UiHarness`]'s synchronous API).
//!
//! The exact selection/interaction techniques here (real `Input.
//! dispatchMouseEvent` presses instead of JS `.click()`, a JS-dispatched
//! `WheelEvent` instead of CDP's own unreliable synthetic wheel, and
//! value-assignment + `dispatchEvent('input')` for typing instead of
//! per-character key events) were all verified against this exact DOM
//! backend in `www/test_dom_e2e.py` before being ported here — see that
//! file's comments for the specific failures each one works around.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::{Message, WebSocket, stream::MaybeTlsStream};

use crate::os::{OSEventFlag, OSKeyCode};

use super::UiDriver;

const CHROME_CANDIDATES: &[&str] = &[
    "chromium",
    "chromium-browser",
    "google-chrome",
    "google-chrome-stable",
    // macOS ships browsers as app bundles, and none of these put a launcher
    // on `PATH` — the binary lives inside the bundle under a name with a
    // space in it, so it is only ever found by absolute path. Without these
    // every `::cdp` test panics with "no local Chromium/Chrome binary found"
    // on a Mac that has Chrome installed.
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
];

fn pick_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind an ephemeral port")
        .local_addr()
        .expect("local_addr")
        .port()
}

fn wait_for_port(port: u16, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        if Instant::now() > deadline {
            panic!("nothing listening on 127.0.0.1:{port} after {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A minimal blocking HTTP GET, used only to poll Chrome's `/json` endpoint
/// for its per-tab WebSocket debugger URL — not a general HTTP client.
///
/// Reads with a short timeout instead of `read_to_end`'s wait-for-EOF: our
/// `Connection: close` request header is only a request, and Chrome's
/// DevTools HTTP server doesn't actually honor it (it keeps the connection
/// open), so `read_to_end` would block forever with no error at all — this
/// was found as a genuine hang, not a guessed-at edge case.
fn http_get_json(port: u16, path: &str) -> Option<Value> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok()?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            // A timeout (or any other error) just means "no more data is
            // coming soon" — use whatever arrived so far.
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let body = text.split_once("\r\n\r\n")?.1;
    serde_json::from_str(body).ok()
}

/// A background static file server for the wasm bundle — a spawned
/// `python3 -m http.server`, same as `www/test_dom_e2e.py` uses, rather than
/// a hand-rolled Rust server: it only needs to exist for the lifetime of one
/// test process and Python is already a build-time prerequisite for `mae`'s
/// wasm demos (`www/build.sh`'s own usage instructions run one the same
/// way).
struct StaticServer(Child);

impl Drop for StaticServer {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn spawn_static_server(dir: &Path, port: u16) -> StaticServer {
    let child = Command::new("python3")
        .args([
            "-m",
            "http.server",
            &port.to_string(),
            "--bind",
            "127.0.0.1",
            "--directory",
        ])
        .arg(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn `python3 -m http.server` (python3 must be on PATH)");
    wait_for_port(port, Duration::from_secs(5));
    StaticServer(child)
}

struct ChromeProcess(Child);

impl Drop for ChromeProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn launch_chromium(cdp_port: u16) -> ChromeProcess {
    for bin in CHROME_CANDIDATES {
        let spawned = Command::new(bin)
            .args([
                "--headless=new",
                "--disable-gpu",
                "--no-sandbox",
                &format!("--remote-debugging-port={cdp_port}"),
                "--remote-allow-origins=*",
                // Tall enough that most demo/app content is reachable
                // without a real scroll gesture first.
                "--window-size=1000,2700",
                "about:blank",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(child) = spawned {
            return ChromeProcess(child);
        }
    }
    panic!("no local Chromium/Chrome binary found on PATH (tried {CHROME_CANDIDATES:?})");
}

fn wait_for_ws_url(cdp_port: u16, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(Value::Array(tabs)) = http_get_json(cdp_port, "/json") {
            for tab in &tabs {
                if tab.get("type").and_then(Value::as_str) == Some("page")
                    && let Some(url) = tab.get("webSocketDebuggerUrl").and_then(Value::as_str)
                {
                    return url.to_string();
                }
            }
        }
        if Instant::now() > deadline {
            panic!("chromium never exposed a debuggable page target on port {cdp_port}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Block until the app has rendered at least one child into `#mae-root`
/// (i.e. the wasm module loaded and produced its first frame). `what` names
/// the page only for the timeout message.
fn wait_for_first_render(conn: &mut CdpConn, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let n = conn.eval(
            "document.getElementById('mae-root') ? document.getElementById('mae-root').children.length : -1",
        );
        if n.as_i64().unwrap_or(-1) > 0 {
            return;
        }
        assert!(
            Instant::now() <= deadline,
            "wasm module at {what} never rendered anything into #mae-root"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Raw JSON-RPC-over-WebSocket client for the handful of CDP methods this
/// driver needs. Not a general CDP client: events (messages with no `id`,
/// e.g. `Network.*`/`Runtime.consoleAPICalled`) are read and discarded, not
/// dispatched anywhere — nothing here needs them.
struct CdpConn {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    next_id: u64,
    /// Every `console.error` and uncaught exception the page has reported
    /// since the connection opened, oldest first — see
    /// [`CdpDriver::console_errors`].
    errors: Vec<String>,
}

impl CdpConn {
    fn connect(ws_url: &str) -> Self {
        let (ws, _) = tungstenite::connect(ws_url).expect("connect to chrome devtools websocket");
        Self {
            ws,
            next_id: 1,
            errors: Vec::new(),
        }
    }

    /// Record a CDP *event* (a message with no `id`) if it reports something
    /// going wrong in the page. Two kinds count: `Runtime.exceptionThrown`
    /// (an uncaught JS exception, which is also how a wasm panic surfaces
    /// once `console_error_panic_hook` has turned it into a real message)
    /// and a `console.error` call. Everything else — logs, warnings, network
    /// events — is dropped, as it always was.
    fn note_if_error(&mut self, value: &Value) {
        let method = value.get("method").and_then(Value::as_str).unwrap_or("");
        let text = match method {
            "Runtime.exceptionThrown" => {
                let details = &value["params"]["exceptionDetails"];
                let described = details["exception"]["description"]
                    .as_str()
                    .or_else(|| details["exception"]["value"].as_str())
                    .or_else(|| details["text"].as_str())
                    .unwrap_or("uncaught exception");
                described.to_string()
            }
            "Runtime.consoleAPICalled" if value["params"]["type"].as_str() == Some("error") => {
                let args = value["params"]["args"].as_array();
                let joined = args
                    .map(|args| {
                        args.iter()
                            .map(|arg| {
                                arg["value"]
                                    .as_str()
                                    .map(str::to_string)
                                    .or_else(|| arg["description"].as_str().map(str::to_string))
                                    .unwrap_or_else(|| arg["value"].to_string())
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();
                format!("console.error: {joined}")
            }
            _ => return,
        };
        self.errors.push(text);
    }

    fn send(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({ "id": id, "method": method, "params": params }).to_string();
        self.ws
            .send(Message::Text(request.into()))
            .expect("send CDP request");
        loop {
            let msg = self.ws.read().expect("read CDP message");
            let Message::Text(text) = msg else { continue };
            let value: Value =
                serde_json::from_str(text.as_str()).expect("CDP message is valid JSON");
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return value;
            }
            self.note_if_error(&value);
        }
    }

    /// Evaluates `expr` in the page and returns its (`returnByValue`)
    /// result — panics on a thrown JS exception.
    fn eval(&mut self, expr: &str) -> Value {
        let response = self.send(
            "Runtime.evaluate",
            json!({ "expression": expr, "returnByValue": true, "awaitPromise": true }),
        );
        let result = &response["result"];
        if result.get("exceptionDetails").is_some() {
            panic!("CDP JS eval threw: {result}\nexpr: {expr}");
        }
        result["result"]["value"].clone()
    }
}

/// Standard base64 → bytes, for the one caller that needs it
/// ([`CdpDriver::debug_screenshot`] — CDP returns image data base64-encoded).
/// Hand-rolled rather than a dependency: this is the whole of it.
fn base64_decode(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for byte in text.bytes() {
        let Some(value) = ALPHABET.iter().position(|c| *c == byte) else {
            continue; // padding and newlines
        };
        acc = (acc << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// `(code, key, windowsVirtualKeyCode)` for `Input.dispatchKeyEvent`. The
/// third field is required — without it, headless Chrome dispatches the JS-
/// visible `keydown`/`keyup` events (real listeners see them) but never runs
/// its own default editing action for the key (delete a char, move the
/// caret, insert a newline, …), since that's driven by internal logic keyed
/// off the *virtual* key code, not `code`/`key` alone. Found via `debug_
/// backspace`-style investigation: Backspace produced a real keydown/keyup
/// pair with no effect at all on a real `<input>`'s value/selection until
/// this was added.
fn cdp_key(key: OSKeyCode) -> (&'static str, &'static str, u32) {
    use OSKeyCode::*;
    match key {
        KeyA => ("KeyA", "a", 0x41),
        KeyB => ("KeyB", "b", 0x42),
        KeyC => ("KeyC", "c", 0x43),
        KeyD => ("KeyD", "d", 0x44),
        KeyE => ("KeyE", "e", 0x45),
        KeyF => ("KeyF", "f", 0x46),
        KeyG => ("KeyG", "g", 0x47),
        KeyH => ("KeyH", "h", 0x48),
        KeyI => ("KeyI", "i", 0x49),
        KeyJ => ("KeyJ", "j", 0x4A),
        KeyK => ("KeyK", "k", 0x4B),
        KeyL => ("KeyL", "l", 0x4C),
        KeyM => ("KeyM", "m", 0x4D),
        KeyN => ("KeyN", "n", 0x4E),
        KeyO => ("KeyO", "o", 0x4F),
        KeyP => ("KeyP", "p", 0x50),
        KeyQ => ("KeyQ", "q", 0x51),
        KeyR => ("KeyR", "r", 0x52),
        KeyS => ("KeyS", "s", 0x53),
        KeyT => ("KeyT", "t", 0x54),
        KeyU => ("KeyU", "u", 0x55),
        KeyV => ("KeyV", "v", 0x56),
        KeyW => ("KeyW", "w", 0x57),
        KeyX => ("KeyX", "x", 0x58),
        KeyY => ("KeyY", "y", 0x59),
        KeyZ => ("KeyZ", "z", 0x5A),
        KeyEscape => ("Escape", "Escape", 0x1B),
        KeyEnter | KeyKeypadEnter => ("Enter", "Enter", 0x0D),
        KeyTab => ("Tab", "Tab", 0x09),
        KeyBackspace => ("Backspace", "Backspace", 0x08),
        KeyDelete => ("Delete", "Delete", 0x2E),
        KeySpace => ("Space", " ", 0x20),
        KeyLeftArrow => ("ArrowLeft", "ArrowLeft", 0x25),
        KeyRightArrow => ("ArrowRight", "ArrowRight", 0x27),
        KeyUpArrow => ("ArrowUp", "ArrowUp", 0x26),
        KeyDownArrow => ("ArrowDown", "ArrowDown", 0x28),
        KeyHome => ("Home", "Home", 0x24),
        KeyEnd => ("End", "End", 0x23),
        other => panic!(
            "CdpDriver::key_press: no CDP mapping for {other:?} yet — extend `cdp_key` (mirrors \
             the reverse table in os/wasm.rs's `web_code_to_oskeycode`) as new keys are needed"
        ),
    }
}

/// The browser editing command a clipboard chord stands for, if this is one.
///
/// Chrome resolves a key chord to an editing action through the *platform's*
/// key bindings (on macOS, `NSStandardKeyBindingResponding`), a layer a
/// synthesized `Input.dispatchKeyEvent` never passes through. So a dispatched
/// Cmd+V fires a real `keydown` that any listener sees and then does nothing
/// at all — no `beforeinput`, no paste. CDP's own `commands` parameter is the
/// way to say what the chord means; naming it here is the only way a driven
/// browser can exercise its real copy/paste pipeline.
///
/// Deliberately only the clipboard trio. Select-all and undo/redo are
/// chords the app under test implements itself (see `text_edit.rs`'s
/// `primary` block, and `paint_dom.rs`'s rich-text keydown handler, which
/// `preventDefault`s them) — handing those to the browser as well would
/// apply them twice.
fn editing_command(key: OSKeyCode, flags: OSEventFlag) -> Option<&'static str> {
    if (flags as u32) & (OSEventFlag::command() as u32) == 0 {
        return None;
    }
    match key {
        OSKeyCode::KeyC => Some("copy"),
        OSKeyCode::KeyV => Some("paste"),
        OSKeyCode::KeyX => Some("cut"),
        _ => None,
    }
}

/// The character `key`'s `keyDown` should insert, if any — see `key_press`'s
/// doc comment on why this is what actually triggers Chrome's default
/// editing action instead of just firing DOM events with no visible effect.
fn cdp_key_text(key: OSKeyCode) -> Option<&'static str> {
    match key {
        OSKeyCode::KeyEnter | OSKeyCode::KeyKeypadEnter => Some("\r"),
        _ => None,
    }
}

/// `(code, windowsVirtualKeyCode, shift)` for a *character* (not a named
/// key — see `cdp_key`), standard US-QWERTY, for `type_text`. `code` is
/// just as required here as it is in `cdp_key`: without it, `os/wasm.rs`'s
/// `web_code_to_oskeycode(&e.code())` gets an empty string and drops the
/// event before it ever reaches `is_text_input_target` — so a `type_text`
/// that only set `key`/`text` (no `code`) would *look* like it dispatches a
/// real keystroke without actually exercising that bridge at all, silently
/// defeating the entire reason to use real key events over `Input.
/// insertText` in the first place. Extend as new characters gain a key of
/// their own; anything with no key on this board falls through to the
/// keyless form at the end.
fn cdp_char_key(ch: char) -> (String, u32, bool) {
    match ch {
        'a'..='z' => (
            format!("Key{}", ch.to_ascii_uppercase()),
            ch.to_ascii_uppercase() as u32,
            false,
        ),
        'A'..='Z' => (format!("Key{ch}"), ch as u32, true),
        '0'..='9' => (format!("Digit{ch}"), 0x30 + (ch as u32 - '0' as u32), false),
        ' ' => ("Space".to_string(), 0x20, false),
        '!' => ("Digit1".to_string(), 0x31, true),
        '@' => ("Digit2".to_string(), 0x32, true),
        '#' => ("Digit3".to_string(), 0x33, true),
        '$' => ("Digit4".to_string(), 0x34, true),
        '%' => ("Digit5".to_string(), 0x35, true),
        '^' => ("Digit6".to_string(), 0x36, true),
        '&' => ("Digit7".to_string(), 0x37, true),
        '*' => ("Digit8".to_string(), 0x38, true),
        '(' => ("Digit9".to_string(), 0x39, true),
        ')' => ("Digit0".to_string(), 0x30, true),
        '-' => ("Minus".to_string(), 0xBD, false),
        '_' => ("Minus".to_string(), 0xBD, true),
        '=' => ("Equal".to_string(), 0xBB, false),
        '+' => ("Equal".to_string(), 0xBB, true),
        ',' => ("Comma".to_string(), 0xBC, false),
        '<' => ("Comma".to_string(), 0xBC, true),
        '.' => ("Period".to_string(), 0xBE, false),
        '>' => ("Period".to_string(), 0xBE, true),
        '/' => ("Slash".to_string(), 0xBF, false),
        '?' => ("Slash".to_string(), 0xBF, true),
        ';' => ("Semicolon".to_string(), 0xBA, false),
        ':' => ("Semicolon".to_string(), 0xBA, true),
        '\'' => ("Quote".to_string(), 0xDE, false),
        '"' => ("Quote".to_string(), 0xDE, true),
        '[' => ("BracketLeft".to_string(), 0xDB, false),
        '{' => ("BracketLeft".to_string(), 0xDB, true),
        ']' => ("BracketRight".to_string(), 0xDD, false),
        '}' => ("BracketRight".to_string(), 0xDD, true),
        '\\' => ("Backslash".to_string(), 0xDC, false),
        '|' => ("Backslash".to_string(), 0xDC, true),
        '`' => ("Backquote".to_string(), 0xC0, false),
        '~' => ("Backquote".to_string(), 0xC0, true),
        // Anything with no key on a US-QWERTY board — an accented letter, an
        // emoji — is dispatched the way a real device produces one: with the
        // text but no physical key behind it (an emoji picker, a compose key
        // and an IME commit all arrive this way). Chrome inserts on `text`
        // alone, and a hosted field is where such a character can land in the
        // first place: `os/wasm.rs` deliberately leaves typing inside one to
        // the browser, so no `code` is needed for it to be seen. A *named*
        // key still needs its `code` for the reasons above, which is why this
        // fallback is limited to characters.
        other => (String::new(), 0, other.is_uppercase()),
    }
}

/// mae's `OSEventFlag` bit layout (Control=1, Alt=2, Shift=4, Super=8) isn't
/// CDP's (Alt=1, Control=2, Meta=4, Shift=8) — same four modifiers, different
/// bit assignments — so this can't just reuse the raw value.
fn cdp_modifiers(flags: OSEventFlag) -> u32 {
    let bits = flags as u32;
    let mut cdp = 0;
    if bits & (OSEventFlag::Control as u32) != 0 {
        cdp |= 2;
    }
    if bits & (OSEventFlag::Alt as u32) != 0 {
        cdp |= 1;
    }
    if bits & (OSEventFlag::Shift as u32) != 0 {
        cdp |= 8;
    }
    if bits & (OSEventFlag::Super as u32) != 0 {
        cdp |= 4;
    }
    cdp
}

/// Drives a real page in a locally installed headless Chromium over the
/// Chrome DevTools Protocol. See the module doc comment for why raw CDP
/// rather than a full client crate, and [`UiDriver`] for the shared action
/// surface this implements.
pub struct CdpDriver {
    conn: CdpConn,
    settle: Duration,
    _server: StaticServer,
    _chrome: ChromeProcess,
}

impl CdpDriver {
    /// Serves `serve_dir` over a local static HTTP server, launches a local
    /// headless Chromium, navigates it to `http://127.0.0.1:<port><url_path>`,
    /// and waits for `#mae-root` to render at least one child (i.e. the wasm
    /// module has loaded and produced its first frame) before returning.
    ///
    /// Requires `python3`, and one of chromium/chromium-browser/google-chrome
    /// on `PATH` — no browser is downloaded (see the earlier discussion of
    /// `chromiumoxide`'s optional fetcher, which this driver doesn't use).
    pub fn launch(serve_dir: &Path, url_path: &str) -> Self {
        let http_port = pick_free_port();
        let cdp_port = pick_free_port();
        let server = spawn_static_server(serve_dir, http_port);
        let chrome = launch_chromium(cdp_port);
        let ws_url = wait_for_ws_url(cdp_port, Duration::from_secs(10));
        let mut conn = CdpConn::connect(&ws_url);
        conn.send("Runtime.enable", json!({}));
        conn.send("Page.enable", json!({}));
        let url = format!("http://127.0.0.1:{http_port}{url_path}");
        conn.send("Page.navigate", json!({ "url": url }));

        wait_for_first_render(&mut conn, &url);

        let mut driver = Self {
            conn,
            settle: Duration::ZERO,
            _server: server,
            _chrome: chrome,
        };
        driver.install_frame_tracker();
        driver
    }

    /// Reload the page and wait for the app to render again — the browser
    /// equivalent of quitting and relaunching, and the only way to test
    /// what a real user's refresh does to state that is supposed to
    /// persist (browser storage survives this; everything in wasm memory
    /// does not).
    ///
    /// A fresh [`Self::launch`] would *not* test the same thing: every
    /// launch starts a new Chromium with an empty profile, so no
    /// `localStorage`/IndexedDB written by the previous one is visible.
    pub fn reload(&mut self) {
        self.conn
            .send("Page.reload", json!({ "ignoreCache": false }));
        wait_for_first_render(&mut self.conn, "the reloaded page");
        // A reload discards the page's JS context, taking the rAF wrapper
        // `settle` depends on with it — see `install_frame_tracker`.
        self.install_frame_tracker();
        self.settle();
    }

    /// Resize the page's viewport, so a scenario can exercise a layout that
    /// only appears at a given size.
    ///
    /// Chrome is launched at one fixed window size (see `launch_chromium`),
    /// which until this existed made every browser-driven scenario run at
    /// that size no matter what it declared — `driver_test!` passed its
    /// width/height to the native driver and silently dropped them here.
    /// `Emulation.setDeviceMetricsOverride` sets the layout viewport
    /// directly, independent of the real window, and mae picks the change up
    /// on its own: `run_dom` polls the container's box every tick and calls
    /// `IMUI::resize` when it differs (see `imui/lifecycle.rs`).
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.set_device_metrics(width, height, 1.0, false);
        self.settle();
    }

    /// Emulate a touch phone: viewport size, a realistic device pixel ratio,
    /// and real touch input.
    ///
    /// `mobile: true` is what makes Chrome honour the page's `<meta
    /// name="viewport">` instead of taking the width literally — so this,
    /// unlike [`Self::set_viewport`], actually tests that the tag is there
    /// and says the right thing. It also enables `pointer: coarse` and the
    /// mobile layout heuristics, which is what a phone really does.
    pub fn emulate_mobile_device(&mut self, width: f32, height: f32) {
        self.set_device_metrics(width, height, 3.0, true);
        self.conn.send(
            "Emulation.setTouchEmulationEnabled",
            json!({ "enabled": true, "maxTouchPoints": 5 }),
        );
        self.settle();
    }

    fn set_device_metrics(&mut self, width: f32, height: f32, scale: f64, mobile: bool) {
        self.conn.send(
            "Emulation.setDeviceMetricsOverride",
            json!({
                "width": width.round().max(1.0) as i64,
                "height": height.round().max(1.0) as i64,
                "deviceScaleFactor": scale,
                "mobile": mobile,
            }),
        );
    }

    /// A one-finger drag across `id`, as real touch events.
    ///
    /// Not a mouse drag with a different name: `Input.dispatchTouchEvent` is
    /// what produces `pointerType: "touch"` pointer events, which is exactly
    /// the distinction the app under test keys off (a touch drag scrolls
    /// where a mouse drag selects — see `os/wasm.rs`). Needs
    /// [`Self::emulate_mobile_device`] first, or Chrome has no touch screen
    /// to dispatch to.
    pub fn touch_drag(&mut self, id: &str, dx: f32, dy: f32) {
        let (x, y) = self
            .center_of(id)
            .unwrap_or_else(|| panic!("no element with data-mae-id={id:?}"));
        let point = |x: f64, y: f64| json!([{ "x": x, "y": y }]);
        self.conn.send(
            "Input.dispatchTouchEvent",
            json!({ "type": "touchStart", "touchPoints": point(x, y) }),
        );
        self.settle();
        // Several moves, not one: a single jump past the slop threshold is
        // indistinguishable from a teleport, and real momentum/threshold
        // logic is written against a stream of small deltas.
        for step in 1..=4 {
            let f = f64::from(step) / 4.0;
            self.conn.send(
                "Input.dispatchTouchEvent",
                json!({
                    "type": "touchMove",
                    "touchPoints": point(x + f64::from(dx) * f, y + f64::from(dy) * f)
                }),
            );
            self.settle();
        }
        self.conn.send(
            "Input.dispatchTouchEvent",
            json!({ "type": "touchEnd", "touchPoints": json!([]) }),
        );
        self.settle();
    }

    /// Extra fixed delay added after every action's frame sync (see
    /// [`Self::settle`]), for a runner slow enough that even a repainted
    /// frame isn't enough. Defaults to zero — the frame sync is normally
    /// exact, and *far* faster than any sleep long enough to be safe.
    pub fn set_settle_time(&mut self, duration: Duration) {
        self.settle = duration;
    }

    /// Make "is a rebuild still pending?" observable to [`Self::settle`], by
    /// wrapping the page's `requestAnimationFrame` in a counter of
    /// callbacks scheduled but not yet run. mae's DOM backend schedules
    /// exactly one rAF per pending rebuild (`waker.wake()`), so that count
    /// reaching zero is precisely "the page has caught up".
    ///
    /// `__maeRawRaf` keeps the *unwrapped* function so `settle`'s own
    /// polling frames don't inflate the very count they're waiting on.
    /// Installed once per page; re-running it is a no-op, so a caller that
    /// isn't sure whether the page reloaded can just call it again.
    fn install_frame_tracker(&mut self) {
        self.conn.eval(
            "(() => { \
               if (window.__maeRawRaf) return 0; \
               const raw = window.requestAnimationFrame.bind(window); \
               window.__maeRawRaf = raw; \
               window.__maePendingRaf = 0; \
               window.requestAnimationFrame = (cb) => { \
                 window.__maePendingRaf++; \
                 return raw((t) => { window.__maePendingRaf--; cb(t); }); \
               }; \
               return 0; \
             })()",
        );
    }

    /// Wait for the page to actually finish reacting to the action just
    /// dispatched, by riding its own render loop rather than sleeping a
    /// fixed duration.
    ///
    /// Wait until the page has actually finished reacting to whatever was
    /// just dispatched — no fixed delay, and no fixed number of frames.
    ///
    /// mae's DOM backend renders lazily: an event handler stages its work
    /// (`pending_edits`/`pending_selection`/…) and calls `waker.wake()`,
    /// which schedules a `requestAnimationFrame` that does the actual
    /// rebuild. So the thing to wait for is precisely "no rebuild is still
    /// scheduled", which [`Self::install_frame_tracker`] makes observable by
    /// counting outstanding rAF callbacks. This loop yields a frame at a
    /// time until that count has been zero across *two consecutive* frames,
    /// extending for as long as work keeps being scheduled.
    ///
    /// Two, not one, because several of the things worth waiting for are
    /// announced asynchronously and so are not yet pending at the instant
    /// this is called: `selectionchange` in particular is queued by the
    /// browser rather than fired synchronously from the `set_base_and_extent`
    /// that caused it, so a caret move only schedules its rebuild a frame
    /// after the keystroke that moved it. Checking once let scenarios read
    /// the DOM before the row they were about to click had been revealed.
    ///
    /// This is the entire cost of a CDP action — the protocol round trip is
    /// ~0.3ms and a DOM query ~0.5ms, against ~16.7ms per frame waited — so
    /// it alone decides how long a scenario takes. Two earlier attempts are
    /// worth not repeating: a flat 500ms `thread::sleep` (~30x a frame,
    /// which put the arrow-key-heavy matrix scenarios into the many-minutes
    /// range and eventually a CI timeout), and a fixed two-frame wait, which
    /// was fast but *raced* — scenarios intermittently read the DOM before a
    /// rebuild had revealed the row they were about to click.
    fn settle(&mut self) {
        self.conn.eval(
            "new Promise(done => { \
               const raf = window.__maeRawRaf || window.requestAnimationFrame.bind(window); \
               let left = 60, calm = 0; \
               const step = () => raf(() => { \
                 calm = (window.__maePendingRaf || 0) === 0 ? calm + 1 : 0; \
                 if (calm >= 2 || left-- <= 0) done(0); else step(); \
               }); \
               step(); \
             })",
        );
        if !self.settle.is_zero() {
            std::thread::sleep(self.settle);
        }
    }

    /// Debug escape hatch: evaluate arbitrary JS in the page and get the
    /// result back. Not part of `UiDriver` — for ad hoc scenario debugging.
    pub fn debug_eval(&mut self, expr: &str) -> Value {
        self.conn.eval(expr)
    }

    /// Debug escape hatch: save a PNG of the page as it currently looks.
    ///
    /// The DOM backend produces *pixels*, and a scenario can only assert on
    /// the DOM tree behind them — a rule that lands an overlay a hundred
    /// pixels off its target is invisible to every assertion while being
    /// glaring on sight. Not part of `UiDriver` (`NativeDriver` has
    /// `png_capture` for the same job).
    pub fn debug_screenshot(&mut self, path: &Path) {
        let response = self
            .conn
            .send("Page.captureScreenshot", json!({ "format": "png" }));
        let data = response["result"]["data"]
            .as_str()
            .expect("Page.captureScreenshot returned no data");
        std::fs::write(path, base64_decode(data)).expect("write screenshot");
    }

    /// Every `console.error` and uncaught exception the page has reported so
    /// far, oldest first.
    ///
    /// A wasm panic reaches the console as an uncaught exception (apps'
    /// `main`/`test_harness` may install `console_error_panic_hook`, which
    /// turns Chrome's opaque "unreachable executed" into the real Rust panic
    /// message), so this is how a scenario catches the app falling over in a
    /// browser at all: a panicked frame does not fail any assertion by
    /// itself — the page simply stops updating, and the scenario reads it as
    /// "the click did nothing".
    ///
    /// Events only arrive while a CDP request is in flight (they are read off
    /// the same socket), so call this *after* the actions being checked, not
    /// mid-gesture; any [`UiDriver`] action or [`Self::debug_eval`] pumps the
    /// queue.
    pub fn console_errors(&mut self) -> Vec<String> {
        // Cheap round trip whose only job is to drain whatever events are
        // already queued on the socket into `errors` before reading it.
        self.conn.eval("0");
        self.conn.errors.clone()
    }

    /// Allow the page to use the async Clipboard API, which is
    /// permission-gated even in headless Chrome — needed by any scenario
    /// that seeds or inspects the *real* system clipboard rather than
    /// simulating one, which is the only way to exercise the browser's own
    /// copy/paste pipeline end to end.
    ///
    /// Note the page must also be focused before `navigator.clipboard`
    /// will resolve (`writeText` rejects with "Document is not focused"
    /// otherwise), so click into the page first.
    pub fn grant_clipboard_access(&mut self) {
        let origin = self
            .conn
            .eval("location.origin")
            .as_str()
            .unwrap_or_default()
            .to_string();
        self.conn.send(
            "Browser.grantPermissions",
            json!({
                "origin": origin,
                "permissions": ["clipboardReadWrite", "clipboardSanitizedWrite"]
            }),
        );
    }

    /// Debug escape hatch: one raw mouse event, no settle, so a caller can
    /// inspect the page *mid-gesture*.
    pub fn debug_mouse_raw(&mut self, kind: &str, x: f64, y: f64) {
        self.dispatch_mouse(kind, x, y, 1);
    }

    fn center_of(&mut self, id: &str) -> Option<(f64, f64)> {
        let id_lit = serde_json::to_string(id).expect("id is representable as a JSON string");
        let value = self.conn.eval(&format!(
            "(() => {{ \
               const el = document.querySelector('[data-mae-id=\"' + CSS.escape({id_lit}) + '\"], [data-mae-key=\"' + CSS.escape({id_lit}) + '\"]'); \
               if (!el) return null; \
               el.scrollIntoView({{block: 'center', inline: 'center'}}); \
               const r = el.getBoundingClientRect(); \
               return {{x: r.left + r.width / 2, y: r.top + r.height / 2}}; \
             }})()"
        ));
        if value.is_null() {
            return None;
        }
        Some((value["x"].as_f64()?, value["y"].as_f64()?))
    }

    /// `(left, width, vertical centre)` of `id` in page coordinates, for
    /// [`UiDriver::drag_x`]'s fractional horizontal positions.
    fn rect_of(&mut self, id: &str) -> Option<(f64, f64, f64)> {
        let id_lit = serde_json::to_string(id).expect("id is representable as a JSON string");
        let value = self.conn.eval(&format!(
            "(() => {{ \
               const el = document.querySelector('[data-mae-id=\"' + CSS.escape({id_lit}) + '\"], [data-mae-key=\"' + CSS.escape({id_lit}) + '\"]'); \
               if (!el) return null; \
               el.scrollIntoView({{block: 'center', inline: 'center'}}); \
               const r = el.getBoundingClientRect(); \
               return {{x: r.left, w: r.width, y: r.top + r.height / 2}}; \
             }})()"
        ));
        if value.is_null() {
            return None;
        }
        Some((
            value["x"].as_f64()?,
            value["w"].as_f64()?,
            value["y"].as_f64()?,
        ))
    }

    fn dispatch_mouse(&mut self, kind: &str, x: f64, y: f64, buttons: u32) {
        self.conn.send(
            "Input.dispatchMouseEvent",
            json!({
                "type": kind, "x": x, "y": y, "button": "left",
                "buttons": buttons, "clickCount": 1
            }),
        );
    }

    fn dispatch_click(&mut self, x: f64, y: f64, button: &str) {
        // Move onto the target first. A real pointer always arrives before it
        // presses, and that arrival is what fires `pointerenter` — so without
        // it every driven click lands on an element the app still believes is
        // un-hovered, and no hover-dependent behaviour (highlight, tooltip, or
        // a bug in how hover is tracked) can be exercised at all.
        self.conn.send(
            "Input.dispatchMouseEvent",
            json!({"type": "mouseMoved", "x": x, "y": y, "buttons": 0}),
        );
        self.conn.send(
            "Input.dispatchMouseEvent",
            json!({"type": "mousePressed", "x": x, "y": y, "button": button, "clickCount": 1}),
        );
        self.conn.send(
            "Input.dispatchMouseEvent",
            json!({"type": "mouseReleased", "x": x, "y": y, "button": button, "clickCount": 1}),
        );
    }
}

impl UiDriver for CdpDriver {
    fn click(&mut self, id: &str) {
        let (x, y) = self
            .center_of(id)
            .unwrap_or_else(|| panic!("no element with data-mae-id={id:?}"));
        self.dispatch_click(x, y, "left");
        self.settle();
    }

    fn hover(&mut self, id: &str) {
        let (x, y) = self
            .center_of(id)
            .unwrap_or_else(|| panic!("no element with data-mae-id={id:?}"));
        self.dispatch_mouse("mouseMoved", x, y, 0);
        self.settle();
    }

    fn right_click(&mut self, id: &str) {
        let (x, y) = self
            .center_of(id)
            .unwrap_or_else(|| panic!("no element with data-mae-id={id:?}"));
        self.dispatch_click(x, y, "right");
        self.settle();
    }

    fn drag_x(&mut self, id: &str, from_frac: f32, to_frac: f32) {
        let (x0, width, y) = self
            .rect_of(id)
            .unwrap_or_else(|| panic!("no element with data-mae-id={id:?}"));
        let (from_x, to_x) = (x0 + width * from_frac as f64, x0 + width * to_frac as f64);
        // Real press/move/move/release, with a rendered frame between each
        // step. The intermediate moves are what make this a drag rather than
        // a click — the browser only extends a selection while the pointer
        // actually moves with the button down, and `buttons: 1` is what
        // marks it as held. The frames matter too: dispatched back to back,
        // the browser had not finished extending the selection to the final
        // position before the release arrived, so the drag came up short
        // (it selected only as far as the *midpoint*). It also mirrors a
        // real drag, which spans many frames.
        self.dispatch_mouse("mousePressed", from_x, y, 1);
        self.settle();
        self.dispatch_mouse("mouseMoved", from_x + (to_x - from_x) / 2.0, y, 1);
        self.settle();
        self.dispatch_mouse("mouseMoved", to_x, y, 1);
        self.settle();
        self.dispatch_mouse("mouseReleased", to_x, y, 1);
        self.settle();
    }

    fn scroll(&mut self, id: &str, delta: f32) {
        let (x, y) = self
            .center_of(id)
            .unwrap_or_else(|| panic!("no element with data-mae-id={id:?}"));
        // `Input.dispatchMouseEvent`'s own "mouseWheel" type does not
        // reliably produce a real `wheel` DOM event in this headless setup
        // (verified in `www/test_dom_e2e.py`) — a JS-dispatched `WheelEvent`
        // does, and fires real listeners the same as a trusted one for
        // everything except CSS `:hover` matching, which scrolling doesn't
        // depend on.
        self.conn.eval(&format!(
            "document.getElementById('mae-root').dispatchEvent(new WheelEvent('wheel', \
             {{bubbles: true, clientX: {x}, clientY: {y}, deltaY: {delta}}}))"
        ));
        self.settle();
    }

    fn key_press(&mut self, key: OSKeyCode) {
        let (code, key_str, vk) = cdp_key(key);
        let mut down =
            json!({"type": "keyDown", "code": code, "key": key_str, "windowsVirtualKeyCode": vk});
        // `text` on the `keyDown` event is what makes Chrome actually insert
        // a character (a plain `keyDown`/`keyUp` pair alone, even with the
        // right virtual key code, fires the DOM events but inserts nothing —
        // found via `debug_enter`-style investigation: Enter produced no "\n"
        // in a real `<textarea>` until this was added). Only Enter needs it
        // here — every other key this driver sends is either non-inserting
        // (Backspace, arrows, Home/End) or goes through `type_text` instead.
        if let Some(text) = cdp_key_text(key) {
            down["text"] = json!(text);
        }
        self.conn.send("Input.dispatchKeyEvent", down);
        self.conn.send(
            "Input.dispatchKeyEvent",
            json!({"type": "keyUp", "code": code, "key": key_str, "windowsVirtualKeyCode": vk}),
        );
        self.settle();
    }

    fn key_press_with_flags(&mut self, key: OSKeyCode, flags: OSEventFlag) {
        let (code, key_str, vk) = cdp_key(key);
        let modifiers = cdp_modifiers(flags);
        let mut down = json!({
            "type": "keyDown", "code": code, "key": key_str, "windowsVirtualKeyCode": vk, "modifiers": modifiers
        });
        if let Some(command) = editing_command(key, flags) {
            down["commands"] = json!([command]);
        }
        // See `key_press`'s identical `text` handling above.
        if let Some(text) = cdp_key_text(key) {
            down["text"] = json!(text);
        }
        self.conn.send("Input.dispatchKeyEvent", down);
        self.conn.send(
            "Input.dispatchKeyEvent",
            json!({"type": "keyUp", "code": code, "key": key_str, "windowsVirtualKeyCode": vk, "modifiers": modifiers}),
        );
        self.settle();
    }

    fn type_text(&mut self, text: &str) {
        // Real per-character `keyDown`/`keyUp` pairs (the same primitive
        // `key_press` uses) — not a single `Input.insertText`/value-
        // assignment shortcut for the whole string. Those *do* land the
        // right text, but only by construction: they call Chrome's insertion
        // pipeline directly, so they can never exercise (or catch a bug in)
        // whatever happens when a *real keystroke* fires — which is exactly
        // how a browser's own `keydown` bridge (`os/wasm.rs`) is reached,
        // and where a real bug lived (`is_text_input_target` not recognizing
        // a `RICH_TEXT_HOST`'s `contenteditable` host meant every real
        // keystroke there landed twice: once via `beforeinput`, once more
        // replayed as a synthetic `OSEvent` into native's own key handling).
        // `text` on `keyDown` is what makes Chrome actually insert that
        // character — see `key_press`'s identical reasoning for Enter. `code`
        // (and `windowsVirtualKeyCode`) matter here for the same reason they
        // do in `cdp_key`/`key_press` — see `cdp_char_key`'s own doc comment.
        for ch in text.chars() {
            let (code, vk, shift) = cdp_char_key(ch);
            let s = ch.to_string();
            let modifiers = if shift {
                cdp_modifiers(OSEventFlag::Shift)
            } else {
                0
            };
            self.conn.send(
                "Input.dispatchKeyEvent",
                json!({
                    "type": "keyDown", "code": code, "key": s, "text": s, "unmodifiedText": s,
                    "windowsVirtualKeyCode": vk, "modifiers": modifiers
                }),
            );
            self.conn.send(
                "Input.dispatchKeyEvent",
                json!({"type": "keyUp", "code": code, "key": s, "windowsVirtualKeyCode": vk, "modifiers": modifiers}),
            );
        }
        self.settle();
    }

    fn text_of(&mut self, id: &str) -> Option<String> {
        let id_lit = serde_json::to_string(id).expect("id is representable as a JSON string");
        // `.innerText` (the `RICH_TEXT_HOST` / generic-element case), not
        // `.textContent` — `emit_layout_line` (text_edit.rs) gives each
        // visual line its own block-level row `<div>` (`paint_div`'s
        // `display: flex`), and `.textContent` does *not* insert a line
        // break between block-level siblings the way rendering does — it
        // would silently concatenate a two-line buffer into one word.
        // `.innerText` is layout-aware and gets this right (found the hard
        // way: a real edit correctly kept a raw newline in the buffer, but
        // reading it back with `.textContent` here made it look like the
        // newline had vanished). Still only accurate when nothing on the
        // current line is actually hidden (plain text, or no markdown syntax
        // present) — a scenario asserting through hidden-marker content
        // needs a different read path than this one either way.
        let value = self.conn.eval(&format!(
            "(() => {{ \
               const el = document.querySelector('[data-mae-id=\"' + CSS.escape({id_lit}) + '\"], [data-mae-key=\"' + CSS.escape({id_lit}) + '\"]'); \
               if (!el) return null; \
               return (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') ? el.value : el.innerText; \
             }})()"
        ));
        value.as_str().map(str::to_string)
    }

    fn settle(&mut self) {
        CdpDriver::settle(self);
    }

    fn focused_id(&mut self) -> Option<String> {
        // `closest`, not the element itself: a `RICH_TEXT_HOST`'s
        // `contenteditable` div carries the id, but the browser can report
        // one of its painted descendant spans as `activeElement` — and the
        // host is what "has focus" means for the app either way.
        let value = self.conn.eval(
            "(() => { \
               const active = document.activeElement; \
               if (!active || !active.closest) return null; \
               const owner = active.closest('[data-mae-key]'); \
               return owner ? owner.getAttribute('data-mae-key') : null; \
             })()",
        );
        value.as_str().map(str::to_string)
    }
}
