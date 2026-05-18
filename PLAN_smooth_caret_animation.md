# PLAN: Smooth Caret Animation for Text Inputs

## Goal

Implement smooth caret movement for line edits / text fields in a GUI framework that currently has no animation system.

The desired effect:

- When the caret moves, it visually interpolates from its previous pixel position to the new pixel position.
- The real caret is drawn at the current logical text cursor position.
- Optionally, a translucent trail is drawn between the animated caret position and the real caret position.
- The feature must work without adding a full general-purpose animation engine first.

## Non-goals

- Do not redesign the text input system.
- Do not implement full widget transitions, layout animation, or timeline/keyframe animation.
- Do not change text editing semantics.
- Do not delay logical cursor movement; only the visual representation is animated.
- Do not animate selection ranges in this first version.

## References / Prior Art

RAD Debugger added optional cursor trails to visualize cursor motion. Its line edit effect is implemented by animating the caret's pixel offset and drawing a translucent trail rectangle between old and new positions.

## Architecture

Add a minimal per-widget scalar animation cache.

The GUI framework needs a small system that maps:

```txt
(widget_id, animation_name) -> animation_state
```

For this feature, the only required animated value is the caret x/y pixel position.

Suggested state:

```c
typedef struct GuiAnimFloat {
    float current;
    float target;
    bool initialized;
} GuiAnimFloat;

typedef struct GuiAnimVec2 {
    Vec2 current;
    Vec2 target;
    bool initialized;
} GuiAnimVec2;
```

Use whichever type matches the existing framework. If line edits are single-line only, `float caret_x` is enough. If multiline text fields exist or will soon exist, use `Vec2 caret_pos`.

## Required Public/Internal API

Add an internal animation helper:

```c
float gui_animate_f32(GuiID owner, String name, float target, float rate, float dt);
Vec2  gui_animate_vec2(GuiID owner, String name, Vec2 target, float rate, float dt);
```

Behavior:

1. Look up animation state by `(owner, name)`.
2. If not initialized, set `current = target`.
3. Set `target = target`.
4. Move `current` toward `target`.
5. Return `current`.

Use exponential smoothing:

```c
float t = 1.0f - expf(-rate * dt);
current = lerp(current, target, t);
```

This is frame-rate independent and avoids hardcoding behavior for 60 FPS.

Recommended default:

```c
rate = 35.0f
```

This gives a quick, responsive motion suitable for typing.

## Frame Lifecycle

The animation cache must be updated every frame.

Add or reuse:

```c
gui_begin_frame(float dt);
gui_end_frame(void);
```

The animation helper requires a valid `dt`.

Clamp `dt` to avoid huge jumps after tab switching, debugging pauses, or window stalls:

```c
dt = clamp(dt, 0.0f, 1.0f / 15.0f);
```

## Cache Lifetime

Animation entries should not live forever.

At the start of a frame, mark all animation entries as unused.

When `gui_animate_*` is called, mark that entry as used.

At the end of the frame, remove entries that have not been used for N frames.

Suggested:

```c
unused_frame_count > 120
```

This avoids leaking animation state for destroyed widgets.

## Line Edit Integration

Find the text input rendering path.

The line edit likely already computes:

- the text rect
- the font
- the visible string
- the caret index
- the caret pixel x position
- the selection rects
- the clip rect

Add this logic near caret drawing.

### Step 1: Compute Logical Caret Position

For a single-line text input:

```c
float caret_target_x =
    text_origin_x + measure_text_width(text_before_caret);
```

For multiline:

```c
Vec2 caret_target_pos =
    text_layout_position_from_index(text, caret_index);
```

The logical caret position must remain exact and immediate.

### Step 2: Animate Visual Caret Position

Single-line version:

```c
float caret_anim_x = gui_animate_f32(
    widget_id,
    "caret_x",
    caret_target_x,
    gui_style.caret_animation_rate,
    gui_io.delta_time
);
```

Multiline version:

```c
Vec2 caret_anim_pos = gui_animate_vec2(
    widget_id,
    "caret_pos",
    caret_target_pos,
    gui_style.caret_animation_rate,
    gui_io.delta_time
);
```

### Step 3: Draw the Real Caret

Always draw the real caret at the logical position:

```c
Rect caret_rect = {
    caret_target_x,
    text_origin_y,
    caret_target_x + caret_width,
    text_origin_y + line_height
};

draw_rect(caret_rect, caret_color);
```

This ensures the caret is never visually wrong after a big jump, mouse click, or focus change.

### Step 4: Draw Optional Trail

If enabled, draw a translucent rectangle between the animated position and the real position.

Single-line example:

```c
float x0 = min(caret_anim_x, caret_target_x);
float x1 = max(caret_anim_x, caret_target_x);

Rect trail_rect = {
    x0,
    text_origin_y,
    x1 + caret_width,
    text_origin_y + line_height
};

Color trail_color = caret_color;
trail_color.a *= 0.25f;

draw_rect(trail_rect, trail_color);
```

Draw order:

1. selection background
2. text
3. caret trail
4. real caret

Depending on the existing renderer, the trail may look better behind text:

1. selection background
2. caret trail
3. text
4. real caret

Choose the version that matches the framework visually.

## Clip / Scissor Behavior

The caret and trail must respect the text field's clip rect.

Before drawing caret/trail:

```c
push_clip_rect(text_clip_rect);
draw trail;
draw caret;
pop_clip_rect();
```

This prevents the trail from leaking outside horizontally scrolled text fields.

## Horizontal Scrolling

Important: animate screen-space caret position, not raw text-space position, unless the framework already handles scroll animation.

For horizontally scrolling line edits:

```c
caret_target_x = text_origin_x
               - horizontal_scroll_px
               + measure_text_width(text_before_caret);
```

Then animate `caret_target_x`.

When the input scroll offset changes because the caret moved, the caret animation should still follow the final screen position.

## Focus Behavior

When a text field gains focus:

- Initialize the animation state to the current caret position.
- Do not animate from zero or from stale state.

When a text field loses focus:

- Either keep state until cache eviction, or delete caret animation state immediately.
- Do not keep drawing the caret unless existing framework behavior already does so.

When focus moves from one text field to another:

- The new text field should not inherit the previous caret position.
- This is why the animation key must include `widget_id`.

## Input Cases to Support

The animation should trigger for:

- typing normal characters
- backspace/delete
- arrow left/right
- home/end
- Ctrl+left/Ctrl+right or word movement
- mouse click positioning
- programmatic cursor changes
- undo/redo if supported
- paste
- cut
- text replacement

No special event hook is necessary if the render code always animates toward the current computed caret position.

## Blink Interaction

Caret blinking should remain separate from caret movement.

Recommended behavior:

- Movement animation controls position.
- Existing blink timer controls caret alpha/visibility.
- On any caret movement or text input, reset blink timer so the caret becomes visible.

Example:

```c
if (caret_index != previous_caret_index) {
    reset_caret_blink(widget_id);
}
```

The trail may ignore blink state and remain visible briefly, or it may follow caret alpha. Prefer keeping the trail visible during movement.

## Reduced Motion / Setting

Add a setting:

```c
bool smooth_caret_enabled;
bool caret_trail_enabled;
float caret_animation_rate;
```

Defaults:

```c
smooth_caret_enabled = true;
caret_trail_enabled = true;
caret_animation_rate = 35.0f;
```

If reduced motion is enabled globally, disable smooth caret and trail:

```c
if (gui_style.reduced_motion) {
    smooth_caret_enabled = false;
    caret_trail_enabled = false;
}
```

When disabled:

```c
caret_anim_x = caret_target_x;
```

## Rendering Details

Recommended caret width:

```c
caret_width = max(1.0f, floorf(dpi_scale));
```

For trail rendering:

- Use a translucent rectangle.
- Clamp minimum alpha.
- Do not draw trail if distance is tiny.

Example:

```c
float distance = fabsf(caret_anim_x - caret_target_x);

if (distance > 0.5f && caret_trail_enabled) {
    draw_trail();
}
```

Optional polish:

- Round the trail corners if the renderer supports rounded rectangles.
- Fade trail alpha based on distance.
- Use stronger alpha near the real caret and weaker alpha near the animated tail if gradient rectangles are supported.

## Minimal Implementation Order

### Phase 1: Animation Cache

Implement:

```c
gui_animate_f32(...)
```

Add storage to GUI context.

Add cache cleanup.

Add tests or assertions for:

- first call initializes to target
- subsequent calls move toward target
- different widget IDs do not share state
- unused entries are eventually removed

### Phase 2: Line Edit Caret Animation

Modify line edit drawing:

1. Compute logical caret x.
2. Call `gui_animate_f32`.
3. Draw real caret at logical x.
4. Draw optional trail between animated x and logical x.
5. Respect clip rect.

### Phase 3: Settings

Add style/config fields:

```c
smooth_caret_enabled
caret_trail_enabled
caret_animation_rate
```

Wire them to existing style/config system.

### Phase 4: Polish

Handle:

- focus gain initialization
- blink reset on caret movement
- large `dt` clamp
- high DPI caret width
- horizontal scrolling
- tiny-distance trail suppression

## Pseudocode

```c
void gui_draw_line_edit(GuiLineEdit *edit, Rect box) {
    GuiID id = edit->id;

    Rect text_rect = get_line_edit_text_rect(box);
    push_clip_rect(text_rect);

    draw_selection_if_any(edit, text_rect);
    draw_text(edit->visible_text, text_rect);

    float caret_target_x =
        text_rect.x0
        - edit->scroll_x
        + measure_text_width(edit->text, 0, edit->caret_index);

    float caret_visual_x = caret_target_x;

    if (gui_style.smooth_caret_enabled && !gui_style.reduced_motion) {
        caret_visual_x = gui_animate_f32(
            id,
            "caret_x",
            caret_target_x,
            gui_style.caret_animation_rate,
            gui_io.delta_time
        );
    }

    float caret_y0 = text_rect.y0;
    float caret_y1 = text_rect.y0 + font_line_height(edit->font);
    float caret_w  = max(1.0f, floorf(gui_io.dpi_scale));

    if (gui_style.caret_trail_enabled && !gui_style.reduced_motion) {
        float dist = fabsf(caret_visual_x - caret_target_x);

        if (dist > 0.5f) {
            float x0 = min(caret_visual_x, caret_target_x);
            float x1 = max(caret_visual_x, caret_target_x);

            Rect trail = rect(x0, caret_y0, x1 + caret_w, caret_y1);

            Color trail_color = gui_style.caret_color;
            trail_color.a *= 0.25f;

            draw_rect(trail, trail_color);
        }
    }

    if (caret_should_be_visible(edit)) {
        Rect caret = rect(
            caret_target_x,
            caret_y0,
            caret_target_x + caret_w,
            caret_y1
        );

        draw_rect(caret, gui_style.caret_color);
    }

    pop_clip_rect();
}
```

## Edge Cases

### Mouse Click Far Away

The caret may jump a large distance. This should animate, but it must not be too slow.

If distance is very large, either:

```c
rate = higher_rate_for_large_distance;
```

or snap when distance exceeds a threshold:

```c
if (distance > 300.0f) {
    caret_visual_x = caret_target_x;
}
```

Prefer not snapping initially unless the animation feels distracting.

### Text Field Reused For Different Data

If the same widget ID is reused for unrelated text fields, animation state may carry over incorrectly.

Fix by ensuring stable unique IDs per widget instance.

### Programmatic Text Replacement

If the whole text field content is replaced, consider resetting caret animation state.

Add helper:

```c
gui_reset_animation(widget_id, "caret_x");
```

Use it when the text buffer identity changes, not on every text edit.

### IME Composition

If IME/preedit text is supported, keep caret animation based on the final visual caret position reported by the text layout system.

Do not animate IME candidate windows.

### Variable Width Fonts

Always compute caret x from measured text layout, not from character count.

### Ligatures / Complex Text

For advanced text shaping, caret position must come from the text layout/shaping engine.

Do not approximate with substring width if the framework supports ligatures, bidi text, or complex scripts.

## Acceptance Criteria

- Typing characters moves the caret smoothly.
- Backspace/delete moves the caret smoothly.
- Arrow key navigation moves the caret smoothly.
- Mouse click repositioning moves the caret smoothly.
- The logical edit cursor updates immediately.
- The real caret is always drawn at the correct final position.
- Trail is clipped inside the text field.
- No animation state leaks after widgets disappear.
- Different text fields do not share caret animation state.
- Turning off `smooth_caret_enabled` restores old behavior.
- Turning off `caret_trail_enabled` keeps smooth movement but removes the trail.
- Reduced-motion mode disables both movement and trail.
- Works at high DPI.
- Does not require a full animation framework.

## Suggested Files To Touch

Adapt these names to the actual codebase:

```txt
gui_context.h/.c        - animation cache storage
gui_animation.h/.c      - gui_animate_f32 / gui_animate_vec2
gui_style.h             - smooth caret settings
gui_line_edit.c         - caret/trail rendering
gui_input.c             - optional blink reset on caret movement
```

## Implementation Notes For Codex

Prefer a small, local change.

Do not introduce dependencies.

Do not implement easing curves as a large abstraction. Exponential smoothing is enough.

Keep the first version simple:

```txt
target caret position -> animated caret position -> translucent trail -> real caret
```

After the minimal version works, polish only where needed.
