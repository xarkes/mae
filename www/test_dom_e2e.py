#!/usr/bin/env python3
"""
End-to-end tests for the DOM render backend (imui/paint_dom.rs, os/wasm.rs,
imui/lifecycle.rs's new_dom/run_dom) that can only be verified against a real
browser — the `#[test]`/`#[wasm_bindgen_test]` suite (src/imui/tests.rs,
tests/testkit.rs) never touches this code at all: it runs headless via
`IMUI::new_for_test`/`with_drawer(None, ...)`, exercising the same
backend-agnostic layout/input/animation logic the DOM backend is built on,
but not the DOM reconciliation, CSS hover delegation, or wasm event bridging
themselves. This file is the DOM-specific complement.

Drives the `mae` demo (src/main.rs) end-to-end via the Chrome DevTools
Protocol against a locally-launched headless Chromium — real mouse input
(not JS-synthesized `dispatchEvent`, which doesn't move the browser's actual
hover-tracking cursor and so can't exercise CSS `:hover`), a static file
server, and DOM/computed-style assertions.

Prerequisites (one-time):
    rustup target add wasm32-unknown-unknown
    cargo install wasm-bindgen-cli --version <matching the wasm-bindgen dep in Cargo.toml>
    chromium (or a Chromium-based browser) on PATH

Usage:
    ./www/build.sh                      # rebuild www/pkg/ from current source
    uv run --with websocket-client --with requests www/test_dom_e2e.py

Exits non-zero if any case fails or the wasm module never renders.
"""

import atexit
import base64
import http.server
import json
import subprocess
import sys
import threading
import time
from pathlib import Path

import requests
import websocket

REPO_ROOT = Path(__file__).resolve().parent.parent
HTTP_PORT = 8199
CDP_PORT = 9199
URL = f"http://localhost:{HTTP_PORT}/www/"

FAILURES = []


def check(name, condition, detail=""):
    status = "PASS" if condition else "FAIL"
    print(f"[{status}] {name}" + (f" — {detail}" if detail and not condition else ""))
    if not condition:
        FAILURES.append(name)


# --- static file server (serves the repo root, so www/ can reach ../assets/) ---


class _QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def start_http_server():
    handler = lambda *a, **kw: _QuietHandler(*a, directory=str(REPO_ROOT), **kw)
    server = http.server.ThreadingHTTPServer(("localhost", HTTP_PORT), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    atexit.register(server.shutdown)
    return server


# --- CDP ---


class CDP:
    def __init__(self, ws_url):
        self.ws = websocket.create_connection(ws_url)
        self.id = 0

    def send(self, method, params=None):
        self.id += 1
        mid = self.id
        self.ws.send(json.dumps({"id": mid, "method": method, "params": params or {}}))
        while True:
            msg = json.loads(self.ws.recv())
            if msg.get("id") == mid:
                return msg

    def eval(self, expr):
        r = self.send(
            "Runtime.evaluate",
            {"expression": expr, "returnByValue": True, "awaitPromise": True},
        )
        result = r.get("result", {}).get("result", {})
        if "exceptionDetails" in r.get("result", {}):
            raise RuntimeError(r["result"]["exceptionDetails"])
        return result.get("value")


def start_chrome():
    proc = subprocess.Popen(
        [
            "chromium",
            "--headless=new",
            "--disable-gpu",
            "--no-sandbox",
            f"--remote-debugging-port={CDP_PORT}",
            "--remote-allow-origins=*",
            # Tall enough to fit the whole single-page demo (the sidebar/tab
            # switcher this demo used to have is gone — everything is one
            # continuously scrollable page now) without needing real scroll
            # gestures just to bring a click target into the viewport.
            "--window-size=1000,2700",
            "about:blank",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    atexit.register(proc.terminate)
    for _ in range(50):
        try:
            tabs = requests.get(f"http://localhost:{CDP_PORT}/json").json()
            for t in tabs:
                if t.get("type") == "page":
                    return CDP(t["webSocketDebuggerUrl"])
        except Exception:
            pass
        time.sleep(0.2)
    raise RuntimeError("chromium never exposed a debuggable page target")


def find_center(c, text):
    return c.eval(
        f"""
        (() => {{
            const els = Array.from(document.getElementById('mae-root').querySelectorAll('div,button,input,textarea'));
            const el = els.find(e => e.textContent && e.textContent.trim() === {json.dumps(text)});
            if (!el) return null;
            const r = el.getBoundingClientRect();
            return {{x: r.left + r.width/2, y: r.top + r.height/2}};
        }})()
        """
    )


def mouse_move(c, x, y):
    c.send("Input.dispatchMouseEvent", {"type": "mouseMoved", "x": x, "y": y})


def mouse_click(c, x, y):
    c.send(
        "Input.dispatchMouseEvent",
        {"type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 1},
    )
    c.send(
        "Input.dispatchMouseEvent",
        {"type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 1},
    )


def click_text(c, text):
    pos = find_center(c, text)
    if pos is None:
        return False
    mouse_click(c, pos["x"], pos["y"])
    return True


def main():
    if not (REPO_ROOT / "www" / "pkg" / "mae.js").exists():
        print("www/pkg/mae.js not found — run ./www/build.sh first", file=sys.stderr)
        sys.exit(2)

    start_http_server()
    c = start_chrome()
    c.send("Runtime.enable")
    c.send("Page.enable")
    c.send("Page.navigate", {"url": URL})
    rendered = False
    for _ in range(50):
        n = c.eval(
            "document.getElementById('mae-root') ? document.getElementById('mae-root').children.length : -1"
        )
        if n and n > 0:
            rendered = True
            break
        time.sleep(0.2)
    check("wasm module loads and renders something into #mae-root", rendered)
    if not rendered:
        print("Aborting remaining checks — nothing rendered.", file=sys.stderr)
        sys.exit(1)

    # --- initial render matches native's seed content ---
    # The demo is one continuously scrollable page (no sidebar/tab switcher
    # — every section renders together), so section content is checked for
    # presence directly instead of clicking through tabs to reveal it.
    check(
        "header shows the app title",
        c.eval(
            "Array.from(document.getElementById('mae-root').querySelectorAll('div,button')).some(d => d.textContent.trim() === 'Mae — GUI framework demo')"
        ),
    )
    for label in ["Layout", "Widgets", "Render", "Scroll"]:
        check(
            f"'{label}' section heading is present",
            c.eval(
                f"Array.from(document.getElementById('mae-root').querySelectorAll('div,button')).some(d => d.textContent.trim() === {json.dumps(label)})"
            ),
        )
    check(
        "the Widgets section's content renders",
        c.eval(
            "Array.from(document.getElementById('mae-root').querySelectorAll('div,button')).some(d => d.textContent.trim() === 'Signals')"
        ),
    )

    # --- textarea: no extra child div, matches native's seed text exactly ---
    textarea_children = c.eval(
        "document.querySelector('#mae-root textarea') ? document.querySelector('#mae-root textarea').children.length : -1"
    )
    check("hosted <textarea> has no child elements", textarea_children == 0, f"got {textarea_children}")

    seed_value = c.eval("document.querySelector('#mae-root textarea').value")
    expected_seed = (
        "Mae is now a GUI framework demo.\n\n"
        "This text area exercises text input, children-sum layout, "
        "parent-percent layout, and draw command generation.\n\n"
        "Click in here and type."
    )
    check(
        "textarea's initial value matches the native seed text",
        seed_value == expected_seed,
        f"got {seed_value!r}",
    )

    # --- line_edit hosted as a real <input> with the right seed value ---
    input_value = c.eval("document.querySelector('#mae-root input') ? document.querySelector('#mae-root input').value : null")
    check("line_edit's initial value matches native ('Edit me')", input_value == "Edit me", f"got {input_value!r}")

    # --- icon buttons use the icon font, not the regular text font ---
    icon_font = c.eval(
        """
        (() => {
            const els = Array.from(document.getElementById('mae-root').querySelectorAll('div,button'));
            const el = els.find(e => e.textContent && e.textContent.trim() === '\\ue88e');
            return el ? getComputedStyle(el).fontFamily : null;
        })()
        """
    )
    check(
        "an icon button's rendered font is the icon font, not the body font",
        icon_font is not None and "Mae Icons" in icon_font,
        f"got {icon_font!r}",
    )

    # --- image widget renders as a real <img> backed by a blob: URL ---
    img_src = c.eval("document.querySelector('#mae-root img') ? document.querySelector('#mae-root img').src : null")
    check(
        "the demo image renders as an <img> with a blob: URL",
        img_src is not None and img_src.startswith("blob:"),
        f"got {img_src!r}",
    )

    # --- typed edit round-trips through Rust state, surviving an unrelated rebuild ---
    c.eval(
        """
        (() => {
            const el = document.querySelector('#mae-root textarea');
            el.focus();
            el.value = 'e2e round trip probe';
            el.dispatchEvent(new Event('input', {bubbles: true}));
        })()
        """
    )
    time.sleep(0.4)
    plus_pos = find_center(c, "+")
    if plus_pos:
        mouse_click(c, plus_pos["x"], plus_pos["y"])
    time.sleep(0.3)
    check(
        "counter increments on a real click",
        c.eval(
            "Array.from(document.getElementById('mae-root').querySelectorAll('div,button')).some(d => d.textContent.trim() === 'Counter: 1')"
        ),
    )
    check(
        "typed textarea edit survives an unrelated rebuild (round-tripped through Rust state, not just left alone in the DOM)",
        c.eval("document.querySelector('#mae-root textarea').value") == "e2e round trip probe",
    )

    # --- hover feedback is CSS-driven and does not trigger a Rust rebuild ---
    mouse_move(c, 5, 5)
    time.sleep(0.2)
    toggle_pos = find_center(c, "Toggle panel")
    inline_before = c.eval(
        "Array.from(document.getElementById('mae-root').querySelectorAll('div,button')).find(d => d.textContent.trim() === 'Toggle panel').style.background"
    )
    mouse_move(c, toggle_pos["x"], toggle_pos["y"])
    time.sleep(0.25)
    inline_after = c.eval(
        "Array.from(document.getElementById('mae-root').querySelectorAll('div,button')).find(d => d.textContent.trim() === 'Toggle panel').style.background"
    )
    computed_after = c.eval(
        "getComputedStyle(Array.from(document.getElementById('mae-root').querySelectorAll('div,button')).find(d => d.textContent.trim() === 'Toggle panel')).backgroundColor"
    )
    check(
        "hovering a button does not rewrite its inline (Rust-driven) background",
        inline_before == inline_after,
        f"{inline_before!r} -> {inline_after!r}",
    )
    check(
        "hovering a button changes its rendered background via CSS :hover",
        computed_after != inline_after,
        f"computed={computed_after!r} inline={inline_after!r}",
    )
    mouse_move(c, 5, 5)
    time.sleep(0.15)

    # --- scrolling: real wheel input moves content and the browser owns the scrollbar ---
    row0_pos = find_center(c, "Row 0")
    if row0_pos:
        # CDP's `Input.dispatchMouseEvent` type "mouseWheel" does not reliably
        # produce a real `wheel` DOM event under this headless setup (verified
        # separately: the listener never fires) — a genuine JS-dispatched
        # WheelEvent does, and (like `pointermove`/`click`) fires real
        # listeners regardless of the trusted/untrusted distinction that only
        # matters for CSS `:hover` matching.
        c.eval(
            f"""
            document.getElementById('mae-root').dispatchEvent(new WheelEvent('wheel', {{
                bubbles: true, clientX: {row0_pos["x"]}, clientY: {row0_pos["y"]}, deltaY: 300
            }}));
            """
        )
    time.sleep(0.3)
    # `Row 0` stays in the DOM even once scrolled out — see `walk_dom`'s doc
    # comment (paint_dom.rs): every `visible` box gets a real DOM node
    # unconditionally, and the *browser's* `overflow: hidden` is the sole
    # visibility mechanism, matching normal web page behavior. So this
    # checks that the wheel scroll moved `Row 0` out of its clipped
    # container's visible bounds, not that the element disappeared.
    scrolled_out = c.eval(
        """
        (() => {
            const row0 = Array.from(document.getElementById('mae-root').querySelectorAll('div,button'))
                .find(d => d.textContent.trim() === 'Row 0');
            if (!row0) return null;
            const container = row0.closest('div[style*="overflow: hidden"]') || row0.parentElement.parentElement;
            const rowRect = row0.getBoundingClientRect();
            const containerRect = container.getBoundingClientRect();
            return rowRect.bottom <= containerRect.top || rowRect.top >= containerRect.bottom;
        })()
        """
    )
    check(
        "mouse wheel scrolls the vertical list (Row 0 scrolled out of its clipped container)",
        scrolled_out is True,
        f"got {scrolled_out!r}",
    )

    native_scrollbars = c.eval(
        """
        Array.from(document.getElementById('mae-root').querySelectorAll('*')).some(el => {
            const cs = getComputedStyle(el);
            return (cs.overflowY === 'auto' || cs.overflowX === 'auto') &&
                (el.scrollHeight > el.clientHeight || el.scrollWidth > el.clientWidth);
        })
        """
    )
    check("scrollable boxes use native DOM overflow", native_scrollbars is True)

    # --- regression: the horizontal strip's scroll wrapper got `overflow:
    # hidden` and `width: 100%` from the same code path as the (correct)
    # vertical-scroll wrapper, clipping it to one viewport-width of content
    # no matter how far it was scrolled — cards past whatever fit in the
    # first screenful were never reachable. See `ensure_scroll_wrapper`
    # (paint_dom.rs): the wrapper must stay `overflow: visible` (clipping is
    # the *outer* box's job) and shrink-wrap to its content on the scrolled
    # axis (`width: max-content`, not `100%`), not just track the outer
    # box's own viewport size. ---
    card_count = c.eval(
        # A card's leaf label and its (single-child) column wrapper both have
        # textContent "Card N" — restrict to leaves so each card counts once.
        "Array.from(document.getElementById('mae-root').querySelectorAll('div,button'))"
        ".filter(e => e.children.length === 0 && /^Card \\d+$/.test(e.textContent.trim())).length"
    )
    check("all 80 horizontal-strip cards exist in the DOM", card_count == 80, f"got {card_count}")

    card79 = find_center(c, "Card 79")
    check("Card 79 (last card) exists before any horizontal scroll", card79 is not None)
    strip_pos = find_center(c, "Card 0")
    if strip_pos:
        # Real elapsed time between dispatches, not just event count, matters
        # here: the scroll offset animates toward its target over several
        # frames (`animate_scroll_offsets`) rather than jumping instantly, so
        # firing all events with no gap between them leaves the animation
        # mid-flight instead of settled at the new (clamped) target.
        for _ in range(120):
            c.eval(
                f"""
                document.getElementById('mae-root').dispatchEvent(new WheelEvent('wheel', {{
                    bubbles: true, clientX: {strip_pos["x"]}, clientY: {strip_pos["y"]}, deltaY: 300, altKey: true
                }}));
                """
            )
            time.sleep(0.03)
        time.sleep(0.3)
    card79_visible = c.eval(
        """
        (() => {
            const el = Array.from(document.getElementById('mae-root').querySelectorAll('div,button'))
                .find(d => d.textContent.trim() === 'Card 79');
            if (!el) return null;
            const r = el.getBoundingClientRect();
            return r.width > 0 && r.right > 0 && r.left < window.innerWidth;
        })()
        """
    )
    check(
        "alt+wheel scrolls the horizontal strip far enough to reach Card 79",
        card79_visible is True,
        f"got {card79_visible!r}",
    )

    # --- DOM child order stays correct ---
    # General sanity check for `reappend_if_needed` (paint_dom.rs): every
    # paint call must re-append its element in logical order each frame, or
    # a reused DOM node stays wherever it was originally appended. This used
    # to be exercised via a multi-hop tab-switching sequence (a DomKey's
    # positional slot could be occupied by a different tab's content from
    # frame to frame); the demo is a single continuously scrollable page now
    # (no tabs to switch), so it's just checked against the steady-state
    # render instead.
    order = c.eval(
        """
        (() => {
            const body = Array.from(document.getElementById('mae-root').querySelectorAll('div,button'))
                .find(d => (d.textContent||'').trim().startsWith('Vertical list ('));
            return Array.from(body.children).map(el => (el.textContent||'').slice(0, 12));
        })()
        """
    )
    expected_order = ["Vertical list", "Row 0", "Horizontal st", "Card 0Card 1C"]
    check(
        "the scroll section's DOM children stay in logical order",
        order is not None and all(order[i].startswith(expected_order[i][:8]) for i in range(4)),
        f"got {order!r}",
    )

    # --- clickable boxes render as real <button> elements, not <div> with an
    # inline `cursor` CSS field ---
    button_tag = c.eval(
        """
        Array.from(document.getElementById('mae-root').querySelectorAll('div,button'))
            .find(e => e.textContent.trim() === 'Toggle panel')
            .tagName.toLowerCase()
        """
    )
    check("a clickable button is a real <button> element, not a <div>", button_tag == "button", f"got {button_tag!r}")

    inline_cursor = c.eval(
        """
        Array.from(document.getElementById('mae-root').querySelectorAll('button'))
            .some(b => b.style.cursor !== '')
        """
    )
    check(
        "no clickable element has a per-frame inline `cursor` CSS field",
        inline_cursor is False,
        f"got {inline_cursor!r}",
    )
    container_cursor = c.eval("document.getElementById('mae-root').style.cursor")
    check(
        "the container itself has no inline `cursor` CSS field either",
        container_cursor == "",
        f"got {container_cursor!r}",
    )

    button_computed_cursor = c.eval(
        """
        getComputedStyle(Array.from(document.getElementById('mae-root').querySelectorAll('button'))
            .find(e => e.textContent.trim() === 'Toggle panel')).cursor
        """
    )
    check(
        "a clickable button still shows a pointer cursor (via the static stylesheet rule)",
        button_computed_cursor == "pointer",
        f"got {button_computed_cursor!r}",
    )

    # --- regression: a Fill-width label inside a ChildrenSum-width row
    # (the content header) rendered with zero width and never appeared in
    # the DOM at all — not just visually squished, genuinely absent. ---
    check(
        "the page title bar (Fill-width label in the header row) renders",
        c.eval(
            "Array.from(document.getElementById('mae-root').querySelectorAll('div,button'))"
            ".some(e => e.textContent.trim() === 'Mae — GUI framework demo')"
        ),
    )

    # --- regression: a real click on a hosted <input>/<textarea> never
    # focused it, because every node was unconditionally re-appended
    # (moved) every single frame, and a currently-focused element loses
    # focus the instant it (or an ancestor) is detached-and-reinserted,
    # even for a no-op move to the same position. ---
    input_pos = c.eval(
        """
        (() => {
            const el = document.querySelector('#mae-root input');
            const r = el.getBoundingClientRect();
            return {x: r.left + r.width / 2, y: r.top + r.height / 2};
        })()
        """
    )
    mouse_click(c, input_pos["x"], input_pos["y"])
    time.sleep(0.2)
    check(
        "a real click focuses the hosted line_edit <input>",
        c.eval("document.activeElement === document.querySelector('#mae-root input')"),
    )
    c.send("Input.dispatchKeyEvent", {"type": "keyDown", "key": "!", "text": "!"})
    c.send("Input.dispatchKeyEvent", {"type": "keyUp", "key": "!", "text": "!"})
    time.sleep(0.3)
    input_value_after_type = c.eval("document.querySelector('#mae-root input').value")
    check(
        "a keystroke after a real click reaches the focused <input>",
        input_value_after_type == "Edit me!",
        f"got {input_value_after_type!r}",
    )

    # --- regression: `style_differs` didn't track text color, so a theme
    # switch that left a label's background/border/etc. unchanged (common
    # for plain labels — transparent on transparent) never wrote the new
    # `color`, leaving it stuck on the old theme's text color. ---
    color_before = c.eval(
        "getComputedStyle(Array.from(document.getElementById('mae-root').querySelectorAll('div,button'))"
        ".find(e => e.textContent.trim() === 'Mae — GUI framework demo')).color"
    )
    click_text(c, "Dark")
    time.sleep(0.3)
    color_after = c.eval(
        "getComputedStyle(Array.from(document.getElementById('mae-root').querySelectorAll('div,button'))"
        ".find(e => e.textContent.trim() === 'Mae — GUI framework demo')).color"
    )
    check(
        "a label's text color updates on a theme switch even when its background doesn't change",
        color_before != color_after,
        f"{color_before!r} -> {color_after!r}",
    )

    # --- Phase 2: native per-element DOM events (click/hover) replace
    # geometry hit-testing for MOUSE_CLICKABLE boxes on this backend — see
    # imui/paint_dom.rs's attach_interactive_listeners and imui/input.rs's
    # `#[cfg(feature = "dom")]` branch of signal_from_key_and_flags. ---

    # A `clickable_row` (MOUSE_CLICKABLE, no DRAW_HOT_EFFECTS) regression-tests
    # the pointer-events gating fix: without it, this box renders with
    # `pointer-events: none` and the browser's hit-test skips it entirely.
    row_pos = find_center(c, "Click anywhere in this row")
    check("the clickable_row demo target is present", row_pos is not None)
    row_pointer_events = c.eval(
        "getComputedStyle(Array.from(document.getElementById('mae-root').querySelectorAll('div,button'))"
        ".find(e => e.textContent.trim() === 'Click anywhere in this row')).pointerEvents"
    )
    check(
        "a clickable_row (no hot-effect styling) still has pointer-events: auto",
        row_pointer_events == "auto",
        f"got {row_pointer_events!r}",
    )
    for i in range(3):
        mouse_click(c, row_pos["x"], row_pos["y"])
        time.sleep(0.3)
    row_hits = c.eval(
        "Array.from(document.getElementById('mae-root').querySelectorAll('div,button'))"
        ".find(e => e.textContent.trim().startsWith('Row hits:')).textContent.trim()"
    )
    check(
        "3 real clicks on a clickable_row each register exactly once (no missed or double clicks)",
        row_hits == "Row hits: 3",
        f"got {row_hits!r}",
    )

    # Native hover (`pointerenter`/`pointerleave`) feeds Rust's own
    # `.hovering()` signal, not just CSS — distinct from the earlier
    # CSS-only hover check. Hover deliberately doesn't wake the render loop
    # (matches the existing skip-rebuild-on-hover-movement optimization), so
    # a harmless click is used to force the rebuild that surfaces it.
    target_pos = find_center(c, "Click target")
    mouse_move(c, target_pos["x"], target_pos["y"])
    time.sleep(0.15)
    mouse_click(c, target_pos["x"], target_pos["y"])
    time.sleep(0.3)
    report = c.eval(
        "Array.from(document.getElementById('mae-root').querySelectorAll('div,button'))"
        ".find(e => e.textContent.trim().startsWith('pressed=')).textContent.trim()"
    )
    check(
        "native pointerenter/pointerleave feed Rust's own .hovering() signal, not just CSS",
        "hover=true" in report,
        f"got {report!r}",
    )
    mouse_move(c, 5, 5)
    time.sleep(0.15)
    # A harmless click elsewhere forces the rebuild that surfaces the
    # cleared hover state (no tabs to hop through anymore to get one) —
    # the counter button doesn't affect layout, so `target_pos` below stays
    # valid.
    plus_pos = find_center(c, "+")
    if plus_pos:
        mouse_click(c, plus_pos["x"], plus_pos["y"])
    time.sleep(0.3)
    report_after = c.eval(
        "Array.from(document.getElementById('mae-root').querySelectorAll('div,button'))"
        ".find(e => e.textContent.trim().startsWith('pressed=')).textContent.trim()"
    )
    check(
        "hover clears via native pointerleave once the pointer moves away",
        "hover=false" in report_after,
        f"got {report_after!r}",
    )

    # Real right-click (contextmenu, with the native menu suppressed) drives
    # RIGHT_CLICKED — a plain `click` listener alone wouldn't see this at
    # all. No right-click has happened anywhere earlier in this script, so
    # the counter should go from 0 straight to 1.
    c.send(
        "Input.dispatchMouseEvent",
        {"type": "mousePressed", "x": target_pos["x"], "y": target_pos["y"], "button": "right", "clickCount": 1},
    )
    c.send(
        "Input.dispatchMouseEvent",
        {"type": "mouseReleased", "x": target_pos["x"], "y": target_pos["y"], "button": "right", "clickCount": 1},
    )
    time.sleep(0.3)
    right_clicks_after = c.eval(
        "Array.from(document.getElementById('mae-root').querySelectorAll('div,button'))"
        ".find(e => e.textContent.trim().startsWith('pressed=')).textContent.trim()"
    )
    check(
        "a real right-click increments the right_clicks counter (RIGHT_CLICKED via contextmenu)",
        "right_clicks=1" in right_clicks_after,
        f"got {right_clicks_after!r}",
    )

    print()
    if FAILURES:
        print(f"{len(FAILURES)} check(s) failed:")
        for f in FAILURES:
            print(f"  - {f}")
        sys.exit(1)
    else:
        print("All DOM backend e2e checks passed.")


if __name__ == "__main__":
    main()
