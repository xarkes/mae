# Mae Future Architecture

This is the direction for evolving Mae toward a more complete, lightweight GUI
framework architecture.

## Direction

Mae should keep its current strengths: custom rendering, small surface area,
retained-ish UI boxes, and simple immediate declaration. The next step is to
separate the framework into clearer subsystems so larger applications do not
end up with application state, widget behavior, layout, events, and drawing all
mixed into `IMUI`.

## Needed Pieces

1. Add an `App` / `Program` abstraction with explicit messages.

   ```rust
   trait App {
       type Message;

       fn update(&mut self, msg: Self::Message);
       fn view(&mut self, ui: &mut IMUI);
   }
   ```

2. Formalize the retained UI tree.

   Required pieces:
   - stable node IDs
   - diff/reconcile pass
   - node lifecycle
   - persistent widget state
   - explicit invalidation when state, layout, or style changes

3. Add damage-based rendering.

   Required pieces:
   - dirty layout regions
   - dirty paint regions
   - redraw only damaged rectangles
   - correct lazy-rendering support
   - caret animation/redraw without repainting the whole frame

4. Add reusable widget/component abstraction.

   Possible shape:

   ```rust
   trait Widget {
       type Message;

       fn layout(&mut self, ctx: &mut LayoutCtx);
       fn event(&mut self, ctx: &mut EventCtx) -> Option<Self::Message>;
       fn paint(&self, ctx: &mut PaintCtx);
   }
   ```

5. Build proper event routing.

   Required pieces:
   - capture and bubble phases
   - focused widget path
   - hovered widget path
   - scroll targeting
   - pointer capture for dragging
   - keyboard navigation

6. Add a style system.

   Required pieces:
   - theme tokens
   - widget defaults
   - style overrides
   - optional lightweight selectors later

7. Treat text as its own subsystem.

   Required pieces:
   - shaping
   - selection
   - cursor movement
   - IME/composition
   - line wrapping
   - scroll offsets
   - glyph cache invalidation
   - accessibility text ranges

8. Complete the platform layer.

   Required pieces:
   - clipboard
   - drag/drop
   - native menus
   - multi-window
   - accessibility APIs
   - IME
   - high-DPI correctness
   - cursor and pointer capture

9. Formalize the renderer/display-list layer.

   Required primitives:
   - `Rect`
   - `TextRun`
   - `Image`
   - `Clip`
   - `Layer`

   Required behavior:
   - cached GPU resources
   - clipping stack
   - rounded borders as first-class primitives

10. Continue cleaning up the public API.

    The handle-chaining API is a good direction. Longer term, declarations
    should move toward typed widgets or `Element<Message>` rather than raw
    mutation spread across application code.

## Priority Order

1. `App<Message>` architecture
2. event routing, focus, and scroll cleanup
3. retained tree and invalidation
4. display list and damage rendering
5. widget trait/component model
6. text subsystem
7. accessibility and platform features

