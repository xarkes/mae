//! Proves the `UiDriver` mechanism (`src/testkit.rs`'s `NativeDriver`/`UiDriver`,
//! `src/testkit/cdp.rs`'s `CdpDriver`): one scenario function, written once
//! against `impl UiDriver`, runs unchanged against both a synchronous
//! in-process harness and a real page in a real browser — see the design
//! discussion this answers (avoiding duplicated test scenarios between
//! testkit and CDP).
//!
//! Each scenario's `native` test always runs. Its `cdp` counterpart
//! (feature = "cdp") drives the *actual* demo app (`src/main.rs`) as served
//! from `www/pkg` — run `./www/build.sh` first, and it needs a local
//! Chromium (`chromium`/`chromium-browser`/`google-chrome`) and `python3` on
//! `PATH`. It's `#[ignore]`d by default (browser/build dependent, like any
//! other real-browser e2e test): `cargo test --features cdp -- --ignored`.
//! `driver_test!` (below) generates both `#[test]`s from one declaration.

#![cfg(feature = "testkit")]

use mae::imui::{IMUI, MarkdownMode, TextAreaOptions, UISize};
use mae::os::{OSEventFlag, OSKeyCode};
use mae::testkit::{NativeDriver, UiDriver};

/// Generates `<name>::native` and (feature = "cdp") `<name>::cdp` `#[test]`s
/// from a scenario fn `name(new_driver: impl FnMut() -> D)`. `new_driver`
/// builds a fresh driver each time it's called — most scenarios call it
/// once, but some (e.g. `text_input_test_all`) need several independent
/// driver instances, which is exactly why this takes a *factory* rather
/// than an already-built driver.
///
/// mae's demo binary isn't a library (see `counter_widget`'s
/// doc comment), so `native`/`cdp` each need their own factory expression
/// here, rather than one shared `render`/state constructor driving both.
macro_rules! driver_test {
    ($name:ident, native: $native_new:expr, cdp: $cdp_new:expr) => {
        mod $name {
            use super::*;

            #[test]
            fn native() {
                super::$name($native_new);
            }

            #[cfg(feature = "cdp")]
            #[test]
            #[ignore = "needs ./www/build.sh run first, plus a local chromium and python3 on PATH"]
            fn cdp() {
                super::$name($cdp_new);
            }
        }
    };
}

/// The one scenario, shared by both backends: click "+" twice, and check
/// the counter label's text after each click. `driver.exists(id)` — "is
/// there currently an element whose text is exactly `id`" — is how both
/// `NativeDriver` (via `UiNodeSnapshot::matches`) and `CdpDriver` (via the
/// `data-mae-id` attribute `paint_dom.rs` mirrors that same string onto)
/// resolve `id`, so this reads identically for either.
fn counter_scenario<D: UiDriver>(mut new_driver: impl FnMut() -> D) {
    let mut driver = new_driver();
    assert!(
        driver.exists("Counter: 0"),
        "expected the initial counter label"
    );
    driver.click("+");
    assert!(
        driver.exists("Counter: 1"),
        "counter should read 1 after one click on +"
    );
    driver.click("+");
    assert!(
        driver.exists("Counter: 2"),
        "counter should read 2 after a second click on +"
    );
}

/// A minimal, self-contained counter widget for the native side — not
/// `src/main.rs`'s real demo (that's a `[[bin]]`, not reachable from an
/// integration test), but the exact same "label + counter, `+` button"
/// shape its own counter demo uses. `counter_scenario`'s `cdp` test drives
/// that real one instead — see the module doc comment for why full zero-
/// duplication of the app-under-test itself needs the app's entry point
/// exposed from a library target which mae's own demo binary isn't (yet).
fn counter_widget(ui: &mut IMUI, counter: &mut usize) {
    ui.row(|ui| {
        ui.label(&format!("Counter: {counter}"));
        let plus = ui
            .button("+", None)
            .width(ui, UISize::Pixels(36.0))
            .height(ui, UISize::Pixels(32.0));
        if plus.clicked() {
            *counter += 1;
        }
    });
}

driver_test!(
    counter_scenario,
    native: || {
        let mut counter = 0usize;
        NativeDriver::new(400.0, 200.0, move |ui| counter_widget(ui, &mut counter))
    },
    cdp: launch_demo
);

/// A `MOUSE_CLICKABLE` container with no button styling of its own (list-row
/// style hit-testing) — mirrors `src/main.rs`'s demo "clickable row" widget
/// under the "Signals" section. Clicking the (non-clickable) label inside it
/// must still register on the row: proves hit-testing/event bubbling resolve
/// to the nearest clickable ancestor for both backends, not just literal
/// `<button>`-shaped elements.
fn clickable_row_scenario<D: UiDriver>(mut new_driver: impl FnMut() -> D) {
    let mut driver = new_driver();
    assert!(
        driver.exists("Row hits: 0"),
        "expected the initial hit count"
    );
    driver.click("Click anywhere in this row");
    assert!(
        driver.exists("Row hits: 1"),
        "row click should increment the hit count"
    );
    driver.click("Click anywhere in this row");
    assert!(
        driver.exists("Row hits: 2"),
        "a second row click should increment it again"
    );
}

/// Native-side reproduction of the same "clickable row + hit count label"
/// shape as `src/main.rs`'s demo — see `counter_widget`'s doc comment for why
/// native gets its own minimal widget instead of driving the real demo too.
fn clickable_row_widget(ui: &mut IMUI, hits: &mut usize) {
    let row = ui.clickable_row("###clickable_row_demo", |ui| {
        ui.label("Click anywhere in this row")
            .width(ui, UISize::Fill);
    });
    row.width(ui, UISize::Pixels(220.0))
        .height(ui, UISize::Pixels(32.0));
    if row.clicked() {
        *hits += 1;
    }
    ui.label(&format!("Row hits: {hits}"));
}

driver_test!(
    clickable_row_scenario,
    native: || {
        let mut hits = 0usize;
        NativeDriver::new(300.0, 200.0, move |ui| clickable_row_widget(ui, &mut hits))
    },
    cdp: launch_demo
);

/// Click into a text box, jump to one edge with keyboard navigation (never a
/// second click — see the scenarios below for why), type, and check the
/// result — for a `LINE_EDIT`. `End`/`Home` are unambiguous for a single
/// line, unlike a multiline box (see `to_document_start`/`to_document_end`).
fn line_edit_insert_and_delete_scenario<D: UiDriver>(mut new_driver: impl FnMut() -> D) {
    let mut driver = new_driver();
    assert!(
        driver.exists("Edit me"),
        "expected the initial line_edit text"
    );

    driver.click("Edit me");
    driver.key_press(OSKeyCode::KeyEnd);
    driver.type_text("!");
    assert!(
        driver.exists("Edit me!"),
        "typed text should land after the caret (End), not always at the start of the buffer"
    );

    driver.key_press(OSKeyCode::KeyBackspace);
    assert!(
        driver.exists("Edit me"),
        "backspace should delete the just-typed character"
    );

    driver.key_press(OSKeyCode::KeyHome);
    driver.type_text(">> ");
    assert!(
        driver.exists(">> Edit me"),
        "typed text should land at the caret (Home), not always at the end of the buffer"
    );
}

fn line_edit_widget(ui: &mut IMUI, buffer: &mut String) {
    ui.line_edit("###line_edit_demo", buffer, false)
        .width(ui, UISize::Pixels(200.0))
        .height(ui, UISize::Pixels(32.0));
}

driver_test!(
    line_edit_insert_and_delete_scenario,
    native: || {
        let mut buffer = String::from("Edit me");
        NativeDriver::new(300.0, 100.0, move |ui| line_edit_widget(ui, &mut buffer))
    },
    cdp: launch_demo
);

/// Seed for the plain multiline scenarios below — short and free of
/// markdown syntax on purpose: short so the "expected text after this edit"
/// is easy to construct and assert exactly (the driver's `exists`/`text_of`
/// only ever match a *whole* current value, never a substring — see
/// `UiDriver`'s doc comment).
const MULTILINE_SEED: &str = "line one\nline two";

/// Seed for the rendered-markdown scenarios below — same shape as
/// `MULTILINE_SEED` (so the same index/length math applies) but a
/// *different* string, deliberately: `UiDriver::click`/`exists` select an
/// element by its current text, and the DOM backend has no separate
/// "select by stable id" (see `paint_dom.rs`'s `data-mae-id` handling), so
/// two on-screen widgets sharing one seed is a real, easy-to-hit test bug —
/// the query silently matches whichever element comes first in DOM order.
/// That's exactly what happened here: every CDP test meant to exercise
/// `RICH_TEXT_HOST` (`###demo_markdown_textarea`, `src/main.rs`) was
/// actually clicking into `###demo_short_textarea`'s plain `<textarea>`
/// instead (same `MULTILINE_SEED` text, earlier in DOM order) — silently
/// never touching the code this test suite most needs to cover — until this
/// seed collision was found and split apart. Syntax-free like `MULTILINE_
/// SEED`, so the rendered-markdown scenario's DOM read (`CdpDriver::
/// text_of`'s `.textContent` fallback) is exactly the raw buffer — nothing
/// is hidden to diverge from it (see that method's own doc comment).
const MARKDOWN_SEED: &str = "note one\nnote two";

/// More key presses than any seed here has visual lines, in one direction —
/// reaching (and harmlessly stopping at) a document edge without needing to
/// know exactly how many presses that takes. Multiline has no doc-start/
/// doc-end key of its own (`Home`/`End` alone are *line*-local — see
/// `text_edit.rs`'s `OSKeyCode::KeyHome`/`KeyEnd` arms), so this is the
/// deterministic way to get there from an arbitrary post-click position.
///
/// Sized to the *longest* document any scenario in this file builds (the
/// matrix cases' 3 lines — see `build_inline_case`), plus margin, and no
/// more: on the CDP backend every one of these presses costs a real
/// rendered frame, and these helpers run on both sides of nearly every
/// assertion, so surplus presses here are the single biggest lever on how
/// long the browser suite takes.
const PRESSES_TO_DOCUMENT_EDGE: usize = 4;

fn to_document_start<D: UiDriver>(driver: &mut D) {
    for _ in 0..PRESSES_TO_DOCUMENT_EDGE {
        driver.key_press(OSKeyCode::KeyUpArrow);
    }
    driver.key_press(OSKeyCode::KeyHome);
}

fn to_document_end<D: UiDriver>(driver: &mut D) {
    for _ in 0..PRESSES_TO_DOCUMENT_EDGE {
        driver.key_press(OSKeyCode::KeyDownArrow);
    }
    driver.key_press(OSKeyCode::KeyEnd);
}

/// Shared by the plain and rendered-markdown multiline scenarios below —
/// same shape as `line_edit_insert_and_delete_scenario`, generalized to a
/// multi-line buffer (append at the very end, delete, prepend at the very
/// start) via `to_document_start`/`to_document_end` instead of `Home`/`End`.
/// Takes `seed` explicitly (not a shared constant): see `MARKDOWN_SEED`'s
/// doc comment for why the plain and rendered-markdown callers must use
/// *different* text, not the same one.
fn multiline_insert_and_delete_scenario<D: UiDriver>(
    seed: &str,
    mut new_driver: impl FnMut() -> D,
) {
    let mut driver = new_driver();
    assert!(driver.exists(seed), "expected the initial multiline text");

    driver.click(seed);
    to_document_end(&mut driver);
    driver.type_text("!");
    let appended = format!("{seed}!");
    assert!(
        driver.exists(&appended),
        "typed text should land at the very end of the document, not somewhere else"
    );

    driver.key_press(OSKeyCode::KeyBackspace);
    assert!(
        driver.exists(seed),
        "backspace should delete the just-typed character"
    );

    to_document_start(&mut driver);
    driver.type_text(">> ");
    let prepended = format!(">> {seed}");
    assert!(
        driver.exists(&prepended),
        "typed text should land at the very start of the document, not somewhere else \
         (this is the exact shape of a real bug this scenario caught: every rebuild after \
         an edit was resetting the DOM caret to a Rust-computed position instead of the \
         browser's own — see paint_dom.rs's `sync_richtext_caret`)"
    );

    // A real newline (Enter), not just single-char insertion — multiline's
    // one genuinely different operation from a line_edit's.
    to_document_end(&mut driver);
    driver.key_press(OSKeyCode::KeyEnter);
    driver.type_text("line three");
    let with_new_line = format!("{prepended}\nline three");
    assert!(
        driver.exists(&with_new_line),
        "Enter should insert a real newline at the caret"
    );
}

fn plain_textarea_widget(ui: &mut IMUI, buffer: &mut String) {
    ui.textarea("###plain_textarea_demo", buffer)
        .width(ui, UISize::Pixels(220.0))
        .height(ui, UISize::Pixels(80.0));
}

fn plain_textarea_insert_and_delete_scenario<D: UiDriver>(new_driver: impl FnMut() -> D) {
    multiline_insert_and_delete_scenario(MULTILINE_SEED, new_driver);
}

driver_test!(
    plain_textarea_insert_and_delete_scenario,
    native: || {
        let mut buffer = String::from(MULTILINE_SEED);
        NativeDriver::new(300.0, 200.0, move |ui| plain_textarea_widget(ui, &mut buffer))
    },
    // Drives `###demo_short_textarea` (`src/main.rs`) — see its own doc
    // comment for why it's separate from the long-seeded `###demo_textarea`
    // `www/test_dom_e2e.py` already asserts the exact value of.
    cdp: launch_demo
);

/// The one scenario that actually exercises `RICH_TEXT_HOST` (`paint_dom.
/// rs`'s `<div contenteditable>` — every other scenario in this file drives
/// a plain `<input>`/`<textarea>`, unaffected by that code at all): same
/// shape as `multiline_insert_and_delete_scenario`, against a `MarkdownMode
/// ::Rendered` textarea. `MARKDOWN_SEED` has no markdown syntax in it on
/// purpose (see its own doc comment), so nothing is hidden and this reduces
/// to exactly the same assertions.
fn markdown_textarea_widget(ui: &mut IMUI, buffer: &mut String) {
    ui.set_markdown_mode(MarkdownMode::Rendered);
    ui.markdown_textarea_with_options(
        "###markdown_textarea_demo",
        buffer,
        TextAreaOptions::default(),
    )
    .width(ui, UISize::Pixels(220.0))
    .height(ui, UISize::Pixels(80.0));
}

fn markdown_textarea_insert_and_delete_scenario<D: UiDriver>(new_driver: impl FnMut() -> D) {
    multiline_insert_and_delete_scenario(MARKDOWN_SEED, new_driver);
}

driver_test!(
    markdown_textarea_insert_and_delete_scenario,
    native: || {
        let mut buffer = String::from(MARKDOWN_SEED);
        NativeDriver::new(300.0, 200.0, move |ui| markdown_textarea_widget(ui, &mut buffer))
    },
    cdp: launch_demo
);

/// Move the caret to raw char offset `index`, deterministically — doc-start
/// (see `to_document_start`) plus `index` individual `RightArrow` presses,
/// never a click or pixel math (a click only ever lands *approximately*
/// where intended, and worse, may not even land at a consistent spot
/// between the native and DOM backends — see `to_document_start`/`to_
/// document_end`'s own doc comment for the same reasoning).
fn goto_index<D: UiDriver>(driver: &mut D, index: usize) {
    to_document_start(driver);
    for _ in 0..index {
        driver.key_press(OSKeyCode::KeyRightArrow);
    }
}

/// Exhaustive character-level edit coverage for a text input: insert,
/// backspace, and forward-delete (`Del`), each at the very start, a middle
/// position, and the very end of `seed` — nine independent checks, the
/// minimum bar for "typing (and deleting) actually works" (the scenarios
/// above only ever appended/prepended/backspaced-the-last-char, so between
/// them they never actually exercised, say, inserting in the middle of
/// existing text, or forward-delete at all).
///
/// `new_driver` builds a fresh driver (a clean widget, seeded with `seed`)
/// for each of the 9 checks below, so no check has to be undone before the
/// next runs — simpler and more robust than reconstructing "what the buffer
/// must currently say" through nine chained mutations on one shared driver.
fn text_input_test_all<D: UiDriver>(seed: &str, mut new_driver: impl FnMut() -> D) {
    let len = seed.chars().count();
    let mid = len / 2;

    for (label, index) in [("the start", 0), ("the middle", mid), ("the end", len)] {
        let mut driver = new_driver();
        driver.click(seed);
        goto_index(&mut driver, index);
        driver.type_text("X");
        let mut expected: String = seed.chars().take(index).collect();
        expected.push('X');
        expected.extend(seed.chars().skip(index));
        assert!(
            driver.exists(&expected),
            "insert at {label} (index {index}) should produce {expected:?}"
        );
    }

    // Backspace at index 0 deletes nothing (there's no preceding char) — so
    // "delete the character at index 0" means placing the caret right after
    // it (index 1) and backspacing it away; "at the end" is the caret's own
    // natural end-of-buffer position, deleting the last character.
    for (label, index) in [("the start", 1), ("the middle", mid), ("the end", len)] {
        let mut driver = new_driver();
        driver.click(seed);
        goto_index(&mut driver, index);
        driver.key_press(OSKeyCode::KeyBackspace);
        let mut expected: String = seed.chars().take(index - 1).collect();
        expected.extend(seed.chars().skip(index));
        assert!(
            driver.exists(&expected),
            "backspace at {label} (caret at index {index}) should produce {expected:?}"
        );
    }

    // Symmetric reasoning for `Del`: forward-deleting at the very end (index
    // == len) deletes nothing (nothing follows the caret there), so "delete
    // the last character" means placing the caret right before it (index
    // len-1) and forward-deleting it.
    for (label, index) in [("the start", 0), ("the middle", mid), ("the end", len - 1)] {
        let mut driver = new_driver();
        driver.click(seed);
        goto_index(&mut driver, index);
        driver.key_press(OSKeyCode::KeyDelete);
        let mut expected: String = seed.chars().take(index).collect();
        expected.extend(seed.chars().skip(index + 1));
        assert!(
            driver.exists(&expected),
            "forward-delete at {label} (caret at index {index}) should produce {expected:?}"
        );
    }
}

fn line_edit_text_input_test_all<D: UiDriver>(new_driver: impl FnMut() -> D) {
    text_input_test_all("Edit me", new_driver);
}

driver_test!(
    line_edit_text_input_test_all,
    native: || {
        let mut buffer = String::from("Edit me");
        NativeDriver::new(300.0, 100.0, move |ui| line_edit_widget(ui, &mut buffer))
    },
    cdp: launch_demo
);

fn plain_textarea_text_input_test_all<D: UiDriver>(new_driver: impl FnMut() -> D) {
    text_input_test_all(MULTILINE_SEED, new_driver);
}

driver_test!(
    plain_textarea_text_input_test_all,
    native: || {
        let mut buffer = String::from(MULTILINE_SEED);
        NativeDriver::new(300.0, 200.0, move |ui| plain_textarea_widget(ui, &mut buffer))
    },
    cdp: launch_demo
);

fn markdown_textarea_text_input_test_all<D: UiDriver>(new_driver: impl FnMut() -> D) {
    text_input_test_all(MARKDOWN_SEED, new_driver);
}

driver_test!(
    markdown_textarea_text_input_test_all,
    native: || {
        let mut buffer = String::from(MARKDOWN_SEED);
        NativeDriver::new(300.0, 200.0, move |ui| markdown_textarea_widget(ui, &mut buffer))
    },
    cdp: launch_demo
);

/// The invariant this checks: typing identical markdown syntax appends the
/// *same* raw buffer content whether the textarea is in `MarkdownMode::
/// Source` (literal markers, a plain `<textarea>` — same DOM path as
/// `plain_textarea_*` above) or `MarkdownMode::Rendered` (hidden markers, a
/// `RICH_TEXT_HOST`'s `<div contenteditable>` — see `paint_dom.rs`). Only
/// the *rendering* is allowed to differ between the two; the underlying
/// buffer must always hold exactly what was typed.
///
/// Each construct types onto its own fresh line (`Enter` first) — `#
/// heading` and `![]()` are only recognized at the start of a line at all
/// (see `text_edit.rs`'s `style_raw_line`/`parse_image_line`), and this
/// keeps every check independent of how the seed line itself might already
/// be styled. Both sides assert against the *same* independently-computed
/// expected string (`{MARKDOWN_SEED}\n{typed}`) rather than comparing one
/// against the other dynamically — simpler, and matches every other
/// scenario in this file.
fn markdown_syntax_scenario<D: UiDriver>(mut new_driver: impl FnMut() -> D) {
    for typed in [
        "# Title",
        "*italic*",
        "_italic_",
        "**bold**",
        "__bold__",
        "![alt](https://link.to.somewhere)",
    ] {
        let mut driver = new_driver();
        assert!(
            driver.exists(MARKDOWN_SEED),
            "expected the initial markdown text"
        );
        driver.click(MARKDOWN_SEED);
        to_document_end(&mut driver);
        driver.key_press(OSKeyCode::KeyEnter);
        driver.type_text(typed);
        let expected = format!("{MARKDOWN_SEED}\n{typed}");
        assert!(
            driver.exists(&expected),
            "typing {typed:?} on its own new line should produce exactly that raw text in \
             the buffer, unchanged — only the DOM *rendering* is allowed to hide markdown \
             syntax markers, never the underlying buffer"
        );
    }
}

fn markdown_syntax_rendered_scenario<D: UiDriver>(new_driver: impl FnMut() -> D) {
    markdown_syntax_scenario(new_driver);
}

driver_test!(
    markdown_syntax_rendered_scenario,
    native: || {
        let mut buffer = String::from(MARKDOWN_SEED);
        NativeDriver::new(300.0, 200.0, move |ui| markdown_textarea_widget(ui, &mut buffer))
    },
    cdp: launch_demo
);

/// `MarkdownMode::Source` counterpart to `markdown_textarea_widget` — same
/// box, opposite mode: literal markers, a plain `<textarea>` on the DOM
/// backend (not `RICH_TEXT_HOST` — see `textarea_impl`'s flag-setting
/// condition), used as the "known good" side of `markdown_syntax_scenario`'s
/// invariant check for native (`launch_demo_source_mode` is the CDP side).
fn markdown_textarea_source_widget(ui: &mut IMUI, buffer: &mut String) {
    ui.set_markdown_mode(MarkdownMode::Source);
    ui.markdown_textarea_with_options(
        "###markdown_textarea_source_demo",
        buffer,
        TextAreaOptions::default(),
    )
    .width(ui, UISize::Pixels(220.0))
    .height(ui, UISize::Pixels(80.0));
}

fn markdown_syntax_source_scenario<D: UiDriver>(new_driver: impl FnMut() -> D) {
    markdown_syntax_scenario(new_driver);
}

driver_test!(
    markdown_syntax_source_scenario,
    native: || {
        let mut buffer = String::from(MARKDOWN_SEED);
        NativeDriver::new(300.0, 200.0, move |ui| markdown_textarea_source_widget(ui, &mut buffer))
    },
    cdp: launch_demo_source_mode
);

/// Where, relative to a line and to the document, an existing markdown
/// construct sits — the position axis of `markdown_construct_matrix_scenario`'s
/// test matrix. `LineStart`/`LineMiddle`/`LineEnd` all place the construct's
/// line as the *middle* line of a 3-line document (so a doc edge is never
/// accidentally also involved); `DocFirstLine`/`DocLastLine` place it as the
/// very first/last line instead (no line above/below at all), always with
/// text on both sides on that line — same shape as `LineMiddle`, just at a
/// document edge. Headers reuse `LineStart` (line_idx 1, offset 0 — headers
/// are only ever recognized at a line's own start, so "middle"/"end" don't
/// apply to them) alongside the two doc-edge variants — see `build_header_case`.
#[derive(Clone, Copy)]
enum Position {
    LineStart,
    LineMiddle,
    LineEnd,
    DocFirstLine,
    DocLastLine,
}

/// How an existing construct gets edited — the action axis of the matrix.
/// `WriteBefore`/`WriteAfter`/`WriteBeforeAndAfter` place the caret purely by
/// keyboard (`goto_index`, absolute from document start — the same
/// no-pixel-math positioning every other scenario in this file already
/// relies on). `ClickBeforeWrite`/`ClickAfterWrite` instead place it via a
/// real mouse click on the construct's own (now-revealed) line followed by a
/// short, deterministic keyboard nudge (`click_then_goto_in_line`) — this is
/// the one part of the matrix that exercises click-to-caret hit-testing
/// itself, a materially different code path on the DOM backend (a browser
/// `Range`/`Selection` translated back to a raw offset) than pure arrow-key
/// navigation.
#[derive(Clone, Copy)]
enum Action {
    WriteBefore,
    WriteAfter,
    WriteBeforeAndAfter,
    ClickBeforeWrite,
    ClickAfterWrite,
}

const ACTIONS: [Action; 5] = [
    Action::WriteBefore,
    Action::WriteAfter,
    Action::WriteBeforeAndAfter,
    Action::ClickBeforeWrite,
    Action::ClickAfterWrite,
];

/// One fully-specified matrix case: a 3-line baseline document containing
/// exactly one markdown construct, plus everything `run_action` needs to
/// locate it — `marker_start`/`marker_end` (raw offsets right before/after
/// the whole construct, header or inline) and `line_start`/`line_text` (for
/// the click-based actions, see `click_then_goto_in_line`).
struct MatrixCase {
    position_label: &'static str,
    doc: String,
    marker_start: usize,
    marker_end: usize,
    line_start: usize,
    line_text: String,
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// Builds a `MatrixCase` for an inline construct (`open`/`close` markers
/// around a one-char content, e.g. `("*", "*")` for italic or `("**", "**")`
/// for bold) at the given `Position`. Non-target lines are short, distinct
/// filler (`xx`/`yy`/`zz`) — long enough to be unambiguous `driver.click`
/// targets, short enough to keep the keyboard navigation in every action
/// cheap (this matrix has a lot of cases — see the module-level scenario
/// functions below).
fn build_inline_case(open: &str, close: &str, position: Position) -> MatrixCase {
    let construct = format!("{open}m{close}");
    let (line_idx, line_text, offset_in_line, position_label) = match position {
        Position::LineStart => (1, format!("{construct} b"), 0, "line start"),
        Position::LineMiddle => (1, format!("a {construct} b"), char_len("a "), "line middle"),
        Position::LineEnd => (1, format!("a {construct}"), char_len("a "), "line end"),
        Position::DocFirstLine => (
            0,
            format!("a {construct} b"),
            char_len("a "),
            "first line of document",
        ),
        Position::DocLastLine => (
            2,
            format!("a {construct} b"),
            char_len("a "),
            "last line of document",
        ),
    };
    let mut lines: Vec<String> = ["xx", "yy", "zz"].iter().map(|s| s.to_string()).collect();
    lines[line_idx] = line_text.clone();
    let doc = lines.join("\n");
    let line_start: usize = lines[..line_idx].iter().map(|l| char_len(l) + 1).sum();
    let marker_start = line_start + offset_in_line;
    let marker_end = marker_start + char_len(&construct);
    MatrixCase {
        position_label,
        doc,
        marker_start,
        marker_end,
        line_start,
        line_text,
    }
}

/// Builds a `MatrixCase` for a header construct (`hashes` is `"#"` or
/// `"##"`) — see `Position`'s doc comment for why only 3 of its 5 variants
/// apply. `marker_start`/`marker_end` span the *whole* raw line (a header
/// has no closing marker to write "after" — the analogue of "after the
/// construct" is the end of its own line).
fn build_header_case(hashes: &str, position: Position) -> MatrixCase {
    let line_text = format!("{hashes} H");
    let (line_idx, position_label) = match position {
        Position::LineStart => (1, "line middle"),
        Position::DocFirstLine => (0, "first line of document"),
        Position::DocLastLine => (2, "last line of document"),
        Position::LineMiddle | Position::LineEnd => {
            unreachable!("headers only ever use LineStart/DocFirstLine/DocLastLine")
        }
    };
    let mut lines: Vec<String> = ["xx", "yy", "zz"].iter().map(|s| s.to_string()).collect();
    lines[line_idx] = line_text.clone();
    let doc = lines.join("\n");
    let line_start: usize = lines[..line_idx].iter().map(|l| char_len(l) + 1).sum();
    let marker_start = line_start;
    let marker_end = line_start + char_len(&line_text);
    MatrixCase {
        position_label,
        doc,
        marker_start,
        marker_end,
        line_start,
        line_text,
    }
}

fn insert_at(s: &str, index: usize, text: &str) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    chars.splice(index..index, text.chars());
    chars.into_iter().collect()
}

/// Removes the `len` chars just inserted at `insert_offset` (i.e. currently
/// occupying `insert_offset..insert_offset+len`) — restores the previous
/// buffer between actions, without needing a fresh driver (or a browser
/// relaunch) per case; see `run_matrix`'s doc comment for why that matters.
fn undo_insert<D: UiDriver>(driver: &mut D, insert_offset: usize, len: usize) {
    goto_index(driver, insert_offset + len);
    for _ in 0..len {
        driver.key_press(OSKeyCode::KeyBackspace);
    }
}

/// The click-based half of the action axis: reveals `case`'s line (arrow-key
/// navigation onto it — hidden markdown markers only show on the line
/// currently under the caret, see `imui.rs`'s `style_raw_line`), then a real
/// `driver.click` on that now-fully-revealed line's own text (landing
/// somewhere in its middle horizontally — `UiDriver::click` always targets
/// an element's center, see `UiHarness::click`), then a short local `Home` +
/// N `RightArrow`s to reach the exact target offset.
///
/// This is the one part of the matrix that exercises click-to-caret
/// placement, and it is deliberately click-*then*-keyboard: `Home` after a
/// click, and per-raw-char arrow motion, are both things the DOM backend
/// has to implement itself for a rich-text host (see
/// `attach_richtext_listeners`'s keydown handler) — so this path covers
/// them where the purely keyboard-driven actions can't.
fn click_then_goto_in_line<D: UiDriver>(driver: &mut D, case: &MatrixCase, target_offset: usize) {
    goto_index(driver, case.line_start);
    driver.click(&case.line_text);
    driver.key_press(OSKeyCode::KeyHome);
    for _ in 0..(target_offset - case.line_start) {
        driver.key_press(OSKeyCode::KeyRightArrow);
    }
}

/// Runs one (construct, position, action) case: performs `action` against
/// `case`, asserts the raw buffer matches the independently-computed
/// expected string, then undoes the edit so the driver is back at `case.doc`
/// for the next action. `Q`/`W` (write-before/write-after's inserted chars)
/// are arbitrary single ASCII letters distinct from the case's own content
/// (`m`/`H`) and filler (`a`/`b`/`xx`/`yy`/`zz`), chosen only so a failure
/// message is easy to read.
fn run_action<D: UiDriver>(driver: &mut D, action: Action, case: &MatrixCase) {
    match action {
        Action::WriteBefore => {
            goto_index(driver, case.marker_start);
            driver.type_text("Q");
            let expected = insert_at(&case.doc, case.marker_start, "Q");
            assert!(
                driver.exists(&expected),
                "{}: writing right before the construct should produce {expected:?}",
                case.position_label
            );
            undo_insert(driver, case.marker_start, 1);
        }
        Action::WriteAfter => {
            goto_index(driver, case.marker_end);
            driver.type_text("W");
            let expected = insert_at(&case.doc, case.marker_end, "W");
            assert!(
                driver.exists(&expected),
                "{}: writing right after the construct should produce {expected:?} — this is \
                 the exact shape of the reported bug (typing `*hello* there`: continuing to \
                 type immediately after a construct's closing marker moved the caret and \
                 corrupted the buffer)",
                case.position_label
            );
            undo_insert(driver, case.marker_end, 1);
        }
        Action::WriteBeforeAndAfter => {
            goto_index(driver, case.marker_start);
            driver.type_text("Q");
            let after_q = insert_at(&case.doc, case.marker_start, "Q");
            let shifted_end = case.marker_end + 1;
            goto_index(driver, shifted_end);
            driver.type_text("W");
            let expected = insert_at(&after_q, shifted_end, "W");
            assert!(
                driver.exists(&expected),
                "{}: writing both right before and right after the construct should produce \
                 {expected:?}",
                case.position_label
            );
            undo_insert(driver, shifted_end, 1);
            undo_insert(driver, case.marker_start, 1);
        }
        Action::ClickBeforeWrite => {
            click_then_goto_in_line(driver, case, case.marker_start);
            driver.type_text("Q");
            let expected = insert_at(&case.doc, case.marker_start, "Q");
            assert!(
                driver.exists(&expected),
                "{}: clicking right before the construct then writing should produce {expected:?}",
                case.position_label
            );
            undo_insert(driver, case.marker_start, 1);
        }
        Action::ClickAfterWrite => {
            click_then_goto_in_line(driver, case, case.marker_end);
            driver.type_text("W");
            let expected = insert_at(&case.doc, case.marker_end, "W");
            assert!(
                driver.exists(&expected),
                "{}: clicking right after the construct then writing should produce {expected:?}",
                case.position_label
            );
            undo_insert(driver, case.marker_end, 1);
        }
    }
}

/// Drives every (position, action) pair in `cases` against a *single* driver
/// instance (one browser launch for the whole matrix on the CDP side, not
/// one per case — see `undo_insert`): resets the field to `case.doc` (go to
/// document start, delete exactly as many chars as the previous case's
/// baseline had, then type the new one) before each case's 5 actions.
fn run_matrix<D: UiDriver>(cases: &[MatrixCase], mut new_driver: impl FnMut() -> D) {
    let mut driver = new_driver();
    assert!(
        driver.exists(MARKDOWN_SEED),
        "expected the initial markdown text"
    );
    driver.click(MARKDOWN_SEED);
    let mut prev_len = MARKDOWN_SEED.chars().count();
    for case in cases {
        // Backspace from the *end*, not forward-`Delete` from the start:
        // forward-deleting a buffer down to nothing passes through a
        // "newline with an empty line on both sides" state right before the
        // last char, and `Delete` there gets permanently stuck (confirmed
        // empirically against the real CDP page — a separate bug from the
        // one this matrix exists to catch, filed rather than fixed here).
        // Backspacing from the end never creates that shape: the seed's one
        // `\n` gets removed while its *preceding* line is still non-empty.
        to_document_end(&mut driver);
        for _ in 0..prev_len {
            driver.key_press(OSKeyCode::KeyBackspace);
        }
        // `type_text` sends each char as a text-input event, which — like
        // real typing — never produces a newline (`Enter` is its own key,
        // not a printable char); split on `\n` and press `Enter` between
        // segments instead, same as `multiline_insert_and_delete_scenario`'s
        // own "real newline" check.
        for (i, line) in case.doc.split('\n').enumerate() {
            if i > 0 {
                driver.key_press(OSKeyCode::KeyEnter);
            }
            driver.type_text(line);
        }
        prev_len = char_len(&case.doc);
        assert!(
            driver.exists(&case.doc),
            "{}: baseline {:?} should round-trip unedited before any action runs",
            case.position_label,
            case.doc
        );
        for action in ACTIONS {
            run_action(&mut driver, action, case);
        }
    }
}

/// The full matrix for `*m*` (italic): all 5 `Position`s × all 5 `Action`s,
/// against `###markdown_textarea_demo` (`markdown_textarea_widget`/
/// `launch_demo` — `MarkdownMode::Rendered`, i.e. `RICH_TEXT_HOST`). Reuses
/// the existing markdown textarea widget/CDP launcher exactly as
/// `markdown_syntax_rendered_scenario` does — this matrix only adds new
/// *cases*, not a new widget.
fn italic_matrix_scenario<D: UiDriver>(new_driver: impl FnMut() -> D) {
    let cases: Vec<MatrixCase> = [
        Position::LineStart,
        Position::LineMiddle,
        Position::LineEnd,
        Position::DocFirstLine,
        Position::DocLastLine,
    ]
    .into_iter()
    .map(|p| build_inline_case("*", "*", p))
    .collect();
    run_matrix(&cases, new_driver);
}

driver_test!(
    italic_matrix_scenario,
    native: || {
        let mut buffer = String::from(MARKDOWN_SEED);
        NativeDriver::new(300.0, 200.0, move |ui| markdown_textarea_widget(ui, &mut buffer))
    },
    cdp: launch_demo
);

/// Same matrix as `italic_matrix_scenario`, for `**m**` (bold).
fn bold_matrix_scenario<D: UiDriver>(new_driver: impl FnMut() -> D) {
    let cases: Vec<MatrixCase> = [
        Position::LineStart,
        Position::LineMiddle,
        Position::LineEnd,
        Position::DocFirstLine,
        Position::DocLastLine,
    ]
    .into_iter()
    .map(|p| build_inline_case("**", "**", p))
    .collect();
    run_matrix(&cases, new_driver);
}

driver_test!(
    bold_matrix_scenario,
    native: || {
        let mut buffer = String::from(MARKDOWN_SEED);
        NativeDriver::new(300.0, 200.0, move |ui| markdown_textarea_widget(ui, &mut buffer))
    },
    cdp: launch_demo
);

/// The header matrix for `# H` (H1): only the 3 `Position`s that make sense
/// for a header (see `build_header_case`) × all 5 `Action`s.
fn title1_matrix_scenario<D: UiDriver>(new_driver: impl FnMut() -> D) {
    let cases: Vec<MatrixCase> = [
        Position::LineStart,
        Position::DocFirstLine,
        Position::DocLastLine,
    ]
    .into_iter()
    .map(|p| build_header_case("#", p))
    .collect();
    run_matrix(&cases, new_driver);
}

driver_test!(
    title1_matrix_scenario,
    native: || {
        let mut buffer = String::from(MARKDOWN_SEED);
        NativeDriver::new(300.0, 200.0, move |ui| markdown_textarea_widget(ui, &mut buffer))
    },
    cdp: launch_demo
);

/// Same as `title1_matrix_scenario`, for `## H` (H2).
fn title2_matrix_scenario<D: UiDriver>(new_driver: impl FnMut() -> D) {
    let cases: Vec<MatrixCase> = [
        Position::LineStart,
        Position::DocFirstLine,
        Position::DocLastLine,
    ]
    .into_iter()
    .map(|p| build_header_case("##", p))
    .collect();
    run_matrix(&cases, new_driver);
}

driver_test!(
    title2_matrix_scenario,
    native: || {
        let mut buffer = String::from(MARKDOWN_SEED);
        NativeDriver::new(300.0, 200.0, move |ui| markdown_textarea_widget(ui, &mut buffer))
    },
    cdp: launch_demo
);

/// Dragging across a line of a `MarkdownMode::Rendered` textarea selects it,
/// and typing then *replaces* the selection rather than inserting alongside
/// it.
///
/// Dragging the full width of the first row (`0.0` → `1.0` of its own box)
/// selects exactly that whole line on either backend, with no dependence on
/// glyph widths or on where within the line a pixel lands — so the expected
/// buffer is the same for both.
///
/// This was completely broken on the DOM backend, in two independent ways
/// (both fixed in `paint_dom.rs`, see there): the painted spans were
/// `pointer-events: none`, so the browser could not hit-test a drag across
/// them at all; and every `selectionchange` during a drag woke a rebuild
/// that re-collapsed the half-made selection to a bare caret, since only
/// the cursor — never the anchor — was carried through
/// `pending_selection`/`sync_richtext_caret`.
fn markdown_drag_select_scenario<D: UiDriver>(mut new_driver: impl FnMut() -> D) {
    let first_line = MARKDOWN_SEED
        .split('\n')
        .next()
        .expect("seed has a first line");

    let mut driver = new_driver();
    assert!(
        driver.exists(MARKDOWN_SEED),
        "expected the initial markdown text"
    );
    driver.click(MARKDOWN_SEED);
    driver.drag_x(first_line, 0.0, 1.0);
    driver.type_text("Z");
    let expected = MARKDOWN_SEED.replacen(first_line, "Z", 1);
    assert!(
        driver.exists(&expected),
        "dragging across the whole first line should select it, so typing replaces it — \
         expected {expected:?}"
    );

    // And the selection is a real range, not just a moved caret: backspace
    // over a fresh one deletes the whole line rather than a single char.
    let mut driver = new_driver();
    driver.click(MARKDOWN_SEED);
    driver.drag_x(first_line, 0.0, 1.0);
    driver.key_press(OSKeyCode::KeyBackspace);
    let emptied = MARKDOWN_SEED.replacen(first_line, "", 1);
    assert!(
        driver.exists(&emptied),
        "backspace over a dragged selection should delete all of it — expected {emptied:?}"
    );
}

driver_test!(
    markdown_drag_select_scenario,
    native: || {
        let mut buffer = String::from(MARKDOWN_SEED);
        NativeDriver::new(300.0, 200.0, move |ui| markdown_textarea_widget(ui, &mut buffer))
    },
    cdp: launch_demo
);

/// Undo/redo in a `MarkdownMode::Rendered` textarea, at the same word
/// granularity native has: a typing run collapses into one step, and
/// whitespace closes the current word so the next one is its own step.
///
/// Run against both backends deliberately — the point is that the DOM
/// backend behaves *identically* to native here, which it did not.
/// Rendered-markdown editing was doubly broken: edits arriving through
/// `beforeinput` recorded no undo state at all (only the `OSEvent` key path
/// did, which hosted elements never reach), and the keystroke never got
/// anywhere either — a rich-text host prevents every edit the browser would
/// make, so the browser's own undo stack is permanently empty and it emits
/// no `historyUndo` to act on. See `attach_richtext_listeners`.
fn markdown_undo_redo_scenario<D: UiDriver>(mut new_driver: impl FnMut() -> D) {
    let undo = OSEventFlag::command();
    let redo = OSEventFlag::command().with(OSEventFlag::Shift);

    let mut driver = new_driver();
    assert!(
        driver.exists(MARKDOWN_SEED),
        "expected the initial markdown text"
    );
    driver.click(MARKDOWN_SEED);
    to_document_end(&mut driver);

    driver.type_text("XYZ");
    let typed = format!("{MARKDOWN_SEED}XYZ");
    assert!(
        driver.exists(&typed),
        "expected the typed run to land first"
    );

    driver.key_press_with_flags(OSKeyCode::KeyZ, undo);
    assert!(
        driver.exists(MARKDOWN_SEED),
        "undo should take the whole typed run back out"
    );

    driver.key_press_with_flags(OSKeyCode::KeyZ, redo);
    assert!(driver.exists(&typed), "redo should put the typed run back");

    // Two more words: each undo should peel off one word, not one keystroke
    // and not everything at once.
    driver.type_text(" alpha beta");
    assert!(driver.exists(&format!("{typed} alpha beta")));
    driver.key_press_with_flags(OSKeyCode::KeyZ, undo);
    assert!(
        driver.exists(&format!("{typed} alpha ")),
        "one undo should remove exactly the last word"
    );
    driver.key_press_with_flags(OSKeyCode::KeyZ, undo);
    assert!(
        driver.exists(&format!("{typed} ")),
        "the next undo should remove the word before it"
    );
}

driver_test!(
    markdown_undo_redo_scenario,
    native: || {
        let mut buffer = String::from(MARKDOWN_SEED);
        NativeDriver::new(300.0, 200.0, move |ui| markdown_textarea_widget(ui, &mut buffer))
    },
    cdp: launch_demo
);

/// Copy and paste through the *real* system clipboard and the browser's own
/// copy/paste pipeline, in a `MarkdownMode::Rendered` textarea.
///
/// `OSEventFlag::command()`, not a hardcoded Control: this drives the
/// *browser's* clipboard shortcut, which is ⌘ on macOS and Ctrl elsewhere.
/// A hardcoded Ctrl made this pass on Linux and fail on every Mac.
///
/// Copying and then pasting *back into the editor* checks both halves in
/// one round trip, and needs no clipboard-read permission (which headless
/// Chrome refuses to grant) — the buffer itself shows what came back.
///
/// CDP-only: there is no system clipboard behind `NativeDriver`, and the
/// native path is separately covered by `imui/tests.rs`'s
/// `textarea_copy_paste_round_trips_through_clipboard`.
///
/// Paste here was completely broken: `attach_richtext_listeners` intercepts
/// `beforeinput` and read the inserted text from `data`, which is *null*
/// for `insertFromPaste` — the content lives on `dataTransfer` — so every
/// Ctrl+V deleted the selection and inserted nothing at all.
#[cfg(feature = "cdp")]
#[test]
#[ignore = "needs ./www/build.sh run first, plus a local chromium and python3 on PATH"]
fn markdown_copy_and_paste_round_trip_cdp() {
    let mut driver = launch_demo();
    driver.grant_clipboard_access();

    assert!(
        driver.exists(MARKDOWN_SEED),
        "expected the initial markdown text"
    );
    driver.click(MARKDOWN_SEED);

    // Copy the whole first line (see `markdown_drag_select_scenario` for why
    // a full-width drag selects exactly that line).
    let first_line = MARKDOWN_SEED
        .split('\n')
        .next()
        .expect("seed has a first line");
    driver.drag_x(first_line, 0.0, 1.0);
    driver.key_press_with_flags(OSKeyCode::KeyC, OSEventFlag::command());

    // Paste it back at the very end. Both halves have to work for this to
    // land: a failed copy pastes something stale, a failed paste inserts
    // nothing.
    to_document_end(&mut driver);
    driver.key_press_with_flags(OSKeyCode::KeyV, OSEventFlag::command());
    let expected = format!("{MARKDOWN_SEED}{first_line}");
    assert!(
        driver.exists(&expected),
        "copy then paste should round-trip the copied line through the real \
         clipboard — expected {expected:?}"
    );
}

/// Launches the real demo (`src/main.rs`) as served from `www/pkg` — shared
/// by every CDP scenario against it.
#[cfg(feature = "cdp")]
fn launch_demo() -> mae::testkit::cdp::CdpDriver {
    use mae::testkit::cdp::CdpDriver;

    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let pkg = repo_root.join("www/pkg/mae.js");
    assert!(
        pkg.exists(),
        "www/pkg/mae.js not found — run ./www/build.sh first"
    );

    CdpDriver::launch(repo_root, "/www/")
}

/// Same as `launch_demo`, but flips `###demo_markdown_textarea` to
/// `MarkdownMode::Source` first (clicking the mode-toggle button next to it
/// in `src/main.rs`) — the demo's global markdown mode otherwise always
/// starts `Rendered`, with no way for a CDP-driven test (unlike
/// `NativeDriver`'s own from-scratch widget closure) to reach `Source` at
/// all.
#[cfg(feature = "cdp")]
fn launch_demo_source_mode() -> mae::testkit::cdp::CdpDriver {
    let mut driver = launch_demo();
    driver.click("Markdown: Rendered");
    driver
}
