use super::*;

/// Maximum number of undo steps retained per textarea. Oldest steps are dropped past
/// this bound so a long editing session can't grow history without limit.
const UNDO_HISTORY_LIMIT: usize = 256;

/// Display height used for an image link that carries no size hint (`h` is the
/// default pinned axis).
const DEFAULT_IMAGE_HEIGHT: f32 = 240.0;
/// Placeholder height/width ratio used before an image's intrinsic size is known.
const IMAGE_PLACEHOLDER_ASPECT: f32 = 0.66;

/// If `raw_line` is exactly an inline image `![alt](TARGET?h=NNN)` (or `?w=`,
/// optionally surrounded by whitespace), return `(resolve_key, size_hint)` where
/// `resolve_key` is `TARGET` minus any query and `size_hint` pins a dimension.
/// `h=` takes precedence over `w=`. The host resolver maps the key
/// (`./blob/<name>`) to pixels.
fn parse_image_line(raw_line: &str) -> Option<(String, Option<(SizeAxis, f32)>)> {
    let trimmed = raw_line.trim();
    let rest = trimmed.strip_prefix("![")?;
    let alt_close = rest.find("](")?;
    let url = rest[alt_close + 2..].strip_suffix(')')?;
    if url.is_empty() || url.contains(char::is_whitespace) {
        return None;
    }
    let (target, query) = match url.split_once('?') {
        Some((target, query)) => (target, Some(query)),
        None => (url, None),
    };
    if target.is_empty() {
        return None;
    }
    let parse = |q: &str, prefix: &str| -> Option<f32> {
        q.split('&').find_map(|param| {
            param
                .strip_prefix(prefix)
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|x| *x > 0.0)
        })
    };
    let size = query.and_then(|q| {
        parse(q, "h=")
            .map(|h| (SizeAxis::Height, h))
            .or_else(|| parse(q, "w=").map(|w| (SizeAxis::Width, w)))
    });
    Some((target.to_string(), size))
}

impl IMUI {
    pub fn line_edit(&mut self, id: &str, buffer: &mut String, masked: bool) -> UIBoxHandle {
        self.line_edit_impl(id, buffer, masked, "")
    }

    /// A `line_edit` that shows muted `placeholder` text while it is empty.
    ///
    /// The placeholder is display-only: it never enters `buffer`, so an
    /// untouched field still reads as empty to the caller rather than as
    /// whatever the hint happened to say.
    pub fn line_edit_with_placeholder(
        &mut self,
        id: &str,
        buffer: &mut String,
        masked: bool,
        placeholder: &str,
    ) -> UIBoxHandle {
        self.line_edit_impl(id, buffer, masked, placeholder)
    }

    fn line_edit_impl(
        &mut self,
        id: &str,
        buffer: &mut String,
        masked: bool,
        placeholder: &str,
    ) -> UIBoxHandle {
        let handle = self.alloc_box(Some(id), UIBoxFlags::LINE_EDIT);
        #[cfg(feature = "dom")]
        self.apply_pending_dom_edit(handle.key(), buffer);
        #[cfg(feature = "dom")]
        self.apply_pending_dom_focus(handle.key());
        self.boxes[handle.idx].pref_size = [UISize::ParentPct(1.0), UISize::Pixels(32.0)];
        self.boxes[handle.idx].padding = Padding::all(7.0);
        self.boxes[handle.idx].style.bg_color = self.theme.input_bg;
        self.boxes[handle.idx].style.border_color = self.theme.border;
        self.boxes[handle.idx].style.corner_radius = self.theme.radius;
        self.apply_click_to_focus(handle);
        self.apply_line_edit_mouse_selection(handle, buffer);
        if self.box_is_focused(handle) {
            let mut text = buffer.clone();
            self.apply_text_input(
                handle,
                buffer,
                &mut text,
                false,
                false,
                TextAreaLineStyle::Plain,
            );
            self.boxes[handle.idx].style.border_color = self.theme.accent;
        }
        // Show the hint only when there is nothing to show instead. Kept out of
        // the focused branch above so typing the first character replaces it.
        if buffer.is_empty() && !placeholder.is_empty() {
            self.set_edit_display_text(handle, placeholder, false);
            let muted = self.theme.text_muted;
            self.text_color(handle, muted);
        } else {
            self.set_edit_display_text(handle, buffer, masked);
        }
        handle
    }

    pub fn textarea(&mut self, id: &str, buffer: &mut String) -> UIBoxHandle {
        self.textarea_with_options(id, buffer, TextAreaOptions::default())
    }

    pub fn textarea_with_options<T: TextEditBuffer>(
        &mut self,
        id: &str,
        buffer: &mut T,
        options: TextAreaOptions,
    ) -> UIBoxHandle {
        self.textarea_impl(id, buffer, options, TextAreaLineStyle::Plain)
    }

    pub fn markdown_textarea_with_options<T: TextEditBuffer>(
        &mut self,
        id: &str,
        buffer: &mut T,
        options: TextAreaOptions,
    ) -> UIBoxHandle {
        self.textarea_impl(id, buffer, options, TextAreaLineStyle::Markdown)
    }

    /// Current caret position (char index) of a text edit box, if it has been
    /// interacted with. For presence/awareness features.
    pub fn textarea_cursor(&self, handle: UIBoxHandle) -> Option<usize> {
        self.text_edit_states
            .get(&handle.key())
            .map(|state| state.cursor)
    }

    /// Current selection of a text edit box as `(anchor, cursor)` char
    /// indices (direction preserved; equal values mean no selection).
    pub fn textarea_selection(&self, handle: UIBoxHandle) -> Option<(usize, usize)> {
        self.text_edit_states
            .get(&handle.key())?
            .selection
            .map(|sel| (sel.anchor, sel.cursor))
    }

    /// Programmatically move a text edit box's caret (and selection anchor),
    /// e.g. to keep it glued to its logical position while collaborative
    /// edits shift the text. Does not scroll the view or restart the caret
    /// blink.
    pub fn set_textarea_cursor(
        &mut self,
        handle: UIBoxHandle,
        cursor: usize,
        selection_anchor: Option<usize>,
    ) {
        let Some(state) = self.text_edit_states.get_mut(&handle.key()) else {
            return;
        };
        state.cursor = cursor;
        state.selection = selection_anchor
            .filter(|anchor| *anchor != cursor)
            .map(|anchor| TextSelection { anchor, cursor });
        state.desired_column = None;
        // The caret was repositioned, not moved by the user: suppress
        // caret-follow scrolling so remote edits never yank the viewport.
        state.scroll_follow_cursor = Some(cursor);
        self.request_repaint();
    }

    /// Move the caret to `cursor`, focus the box, and *reveal* it by letting
    /// caret-follow scrolling bring it into view on this frame's post-layout
    /// pass — used to jump to a search match. Unlike
    /// [`set_textarea_cursor`](Self::set_textarea_cursor), it does not suppress
    /// the scroll. Creates the edit state if the box was never interacted with.
    pub fn reveal_textarea_cursor(&mut self, handle: UIBoxHandle, cursor: usize) {
        self.focus_key = Some(handle.key());
        let state = self.text_edit_states.entry(handle.key()).or_default();
        state.cursor = cursor;
        state.selection = None;
        state.desired_column = None;
        // Differ from `cursor` so `update_focused_textarea_scroll` runs.
        state.scroll_follow_cursor = None;
        self.request_repaint();
    }

    /// Register remote collaborator carets to overlay on a textarea this
    /// frame (colored bar + initial badge at each char index). Call after the
    /// textarea was built; registrations reset every frame.
    pub fn set_remote_carets(&mut self, handle: UIBoxHandle, carets: Vec<RemoteCaret>) {
        if carets.is_empty() {
            self.remote_carets.remove(&handle.key());
        } else {
            self.remote_carets.insert(handle.key(), carets);
        }
    }

    /// Selects how markdown syntax is presented in `markdown_textarea_*` editors.
    pub fn set_markdown_mode(&mut self, mode: MarkdownMode) {
        self.markdown_mode = mode;
    }

    pub fn markdown_mode(&self) -> MarkdownMode {
        self.markdown_mode
    }

    /// Provide decoded RGBA pixels for an inline image keyed by `name` (the
    /// `./blob/<name>` link target). The GPU upload is deferred to the next
    /// paint pass; any previous texture for that name is queued for deletion.
    pub fn provide_image(&mut self, name: impl Into<String>, width: u32, height: u32, rgba: &[u8]) {
        self.provide_image_entry(name, width, height, rgba.to_vec(), None);
    }

    /// Provide an inline image keyed by `name` as already-encoded bytes (e.g.
    /// straight from a blob store, PNG/JPEG unchanged) plus their MIME type,
    /// instead of decoded RGBA. For the DOM backend, which hands these bytes
    /// straight to a `<img src="blob:...">` and lets the browser decode them
    /// — no pixel decode ever happens on the Rust side, and no GPU texture is
    /// ever uploaded for it (`image_texture_for_paint` never applies).
    #[cfg(feature = "dom")]
    pub fn provide_image_encoded(
        &mut self,
        name: impl Into<String>,
        width: u32,
        height: u32,
        mime: &'static str,
        bytes: &[u8],
    ) {
        self.provide_image_entry(name, width, height, bytes.to_vec(), Some(mime));
    }

    fn provide_image_entry(
        &mut self,
        name: impl Into<String>,
        width: u32,
        height: u32,
        pending: Vec<u8>,
        encoded_mime: Option<&'static str>,
    ) {
        let name = name.into();
        let prev = self.images.insert(
            name.clone(),
            ImageEntry {
                width,
                height,
                tex_id: 0,
                pending: Some(pending),
                encoded_mime,
            },
        );
        if let Some(prev) = prev
            && prev.tex_id != 0
        {
            self.images_to_free.push(prev.tex_id);
        }
        self.requested_images.remove(&name);
        self.images_rev = self.images_rev.wrapping_add(1);
    }

    /// Flag an inline image (by its `./blob/<name>` key) as not yet synced, so a
    /// warning badge is drawn on it. Immediate-mode: call each frame the image
    /// should be badged (cleared at `begin_frame`).
    pub fn mark_image_unsynced(&mut self, key: impl Into<String>) {
        self.image_unsynced.insert(key.into());
    }

    /// Take any image bytes pasted into a multiline editor this frame. The host
    /// uploads it to the active note's space and inserts a `./blob/<name>` link.
    pub fn take_pasted_image(&mut self) -> Option<Vec<u8>> {
        // On the DOM backend the bytes come from the browser's own `paste`
        // event instead of `os::clipboard_get_image` (which has no
        // synchronous equivalent there) — see `attach_richtext_listeners`.
        // Checked first only when nothing is already staged natively, so
        // neither source can shadow the other.
        #[cfg(feature = "dom")]
        if self.pasted_image.is_none()
            && let Some(bytes) = self.dom.as_ref().and_then(|dom| dom.take_pasted_image())
        {
            return Some(bytes);
        }
        self.pasted_image.take()
    }

    /// Take a pending inline-image resize `(link_key, new_size)`. The host
    /// rewrites the matching `?h=`/`?w=` of the `![](key?…)` link in the buffer.
    pub fn take_image_resize(&mut self) -> Option<(String, ImageResize)> {
        self.image_resize_out.take()
    }

    /// If `point` lands in the bottom-right resize grip of an inline image in
    /// textarea `idx`, return that image's `(key, control axis, start size)`.
    /// Driven from the textarea's mouse handling because a nested image box
    /// never receives events (the parent textarea consumes them).
    fn image_resize_grip_at(&self, idx: usize, point: Option<Point>) -> Option<ImageDrag> {
        let point = point?;
        let key = self.boxes[idx].key;
        let layout = self.editor_layouts.get(&key)?;
        const GRIP: f32 = 22.0;
        // Only the emitted lines can be under the pointer, and only they have
        // a row box to measure the grip against — `textarea_line_box` skips
        // the rest.
        for line_idx in layout.emitted.clone() {
            let Some(image) = &layout.lines[line_idx].image else {
                continue;
            };
            // The per-line row box holds the image box as its single child.
            let Some(row_idx) = self.textarea_line_box(idx, line_idx) else {
                continue;
            };
            let img_idx = self.boxes[row_idx]
                .children
                .first()
                .copied()
                .unwrap_or(row_idx);
            let rect = self.boxes[img_idx].rect;
            if point.x >= rect.x1 - GRIP
                && point.x <= rect.x1 + 2.0
                && point.y >= rect.y1 - GRIP
                && point.y <= rect.y1 + 2.0
            {
                let start = match image.control {
                    SizeAxis::Width => image.width,
                    SizeAxis::Height => image.height,
                };
                return Some(ImageDrag {
                    key: image.key.clone(),
                    control: image.control,
                    start,
                    press_pos: point,
                });
            }
        }
        None
    }

    /// The link key of the image currently being resize-dragged, if any (used
    /// by paint to keep the grip visible during a drag).
    pub(crate) fn image_resize_drag_key(&self) -> Option<&str> {
        self.image_drag.as_ref().map(|drag| drag.key.as_str())
    }

    /// While an inline-image resize drag is active, emit the new pinned size for
    /// the host to write into the link. Returns true if a resize is in progress
    /// (so the caller skips text selection).
    fn drive_image_resize(&mut self) -> bool {
        let Some(drag) = self.image_drag.as_ref() else {
            return false;
        };
        if let Some(mouse) = self.mouse {
            let out = match drag.control {
                SizeAxis::Width => ImageResize::Width(
                    (drag.start + (mouse.x - drag.press_pos.x)).max(MIN_IMAGE_SIZE),
                ),
                SizeAxis::Height => ImageResize::Height(
                    (drag.start + (mouse.y - drag.press_pos.y)).max(MIN_IMAGE_SIZE),
                ),
            };
            self.image_resize_out = Some((drag.key.clone(), out));
        }
        true
    }

    /// Whether an image has been provided for `name`.
    pub fn has_image(&self, name: &str) -> bool {
        self.images.contains_key(name)
    }

    /// Intrinsic pixel size of a provided image, if available.
    pub fn image_size(&self, name: &str) -> Option<(u32, u32)> {
        self.images.get(name).map(|e| (e.width, e.height))
    }

    /// Read-only access to a registered image's bytes, for the DOM backend to
    /// build a `<img>` src from (see `paint_dom.rs::paint_image`). Unlike
    /// `image_texture_for_paint` (the native/GPU upload path), this never
    /// consumes `pending` — safe because `draw_ui_dom` never calls that path
    /// at all (no GPU texture is ever uploaded when there's no `Drawer`), so
    /// nothing else touches these bytes while the DOM backend is active.
    /// `mime` is `Some` when `bytes` are already-encoded (PNG/JPEG/…, from
    /// `provide_image_encoded`) — pass straight to a `Blob`, no re-encode —
    /// and `None` when they're raw RGBA8 (from `provide_image`), which the
    /// caller must PNG-encode itself before blobbing.
    #[cfg(feature = "dom")]
    pub(crate) fn image_dom_bytes(
        &self,
        name: &str,
    ) -> Option<(u32, u32, Option<&'static str>, &[u8])> {
        let entry = self.images.get(name)?;
        let bytes = entry.pending.as_deref()?;
        Some((entry.width, entry.height, entry.encoded_mime, bytes))
    }

    /// Drain the image link names referenced this frame but not yet provided.
    /// The host decodes them (off the hot path) and calls [`IMUI::provide_image`].
    pub fn take_requested_images(&mut self) -> Vec<String> {
        self.requested_images.drain().collect()
    }

    /// Forget a provided image (e.g. its blob was deleted), queuing its texture
    /// for deletion during the next paint.
    pub fn drop_image(&mut self, name: &str) {
        if let Some(entry) = self.images.remove(name) {
            self.images_rev = self.images_rev.wrapping_add(1);
            if entry.tex_id != 0 {
                self.images_to_free.push(entry.tex_id);
            }
        }
    }

    /// Paint-time resolve for an inline image: free any queued textures, upload
    /// this image's pending RGBA to the GPU on first paint, and return its
    /// `(tex_id, width, height)`. Must be called from the paint pass (the
    /// renderer context is valid there). `tex_id == 0` means not ready.
    pub(crate) fn image_texture_for_paint(&mut self, name: &str) -> Option<(u32, u32, u32)> {
        if !self.images_to_free.is_empty()
            && let Some(drawer) = self.drawer.as_mut()
        {
            for id in std::mem::take(&mut self.images_to_free) {
                drawer.renderer.remove_image_texture(id);
            }
        }
        let (width, height, pending) = {
            let entry = self.images.get_mut(name)?;
            if entry.tex_id != 0 || entry.pending.is_none() {
                return Some((entry.tex_id, entry.width, entry.height));
            }
            (entry.width, entry.height, entry.pending.take().unwrap())
        };
        let tex_id = self
            .drawer
            .as_mut()
            .map(|d| {
                d.renderer
                    .create_image_texture(width as usize, height as usize, &pending)
            })
            .unwrap_or(0);
        if let Some(entry) = self.images.get_mut(name) {
            entry.tex_id = tex_id;
        }
        Some((tex_id, width, height))
    }

    /// Record that `name` is referenced but not yet available, so the host can
    /// fetch + decode it. No-op once the image is provided.
    pub(crate) fn request_image(&mut self, name: &str) {
        if !self.images.contains_key(name) {
            self.requested_images.insert(name.to_string());
        }
    }

    /// Char length of the active IME preedit when `key` is the focused editor (else 0).
    /// The preedit is injected into the display string, so the rendered caret is offset
    /// by this much past the logical buffer caret.
    pub(super) fn focused_preedit_len(&self, key: UiKey) -> usize {
        if self.focus_key == Some(key) {
            self.ime_preedit.as_ref().map_or(0, |p| p.chars().count())
        } else {
            0
        }
    }

    /// Parse + wrap the buffer into visual lines without cursor-revealed markdown markers.
    #[cfg(test)]
    pub(super) fn build_editor_layout(
        &mut self,
        text: &str,
        wrap_width: f32,
        base_font: f32,
        line_style: TextAreaLineStyle,
        md_mode: MarkdownMode,
    ) -> Vec<LayoutLine> {
        self.build_editor_layout_revealing_line(
            text, wrap_width, base_font, line_style, md_mode, None,
        )
        .lines
    }

    pub(super) fn build_editor_layout_revealing_line(
        &mut self,
        text: &str,
        wrap_width: f32,
        base_font: f32,
        line_style: TextAreaLineStyle,
        md_mode: MarkdownMode,
        reveal_line_start: Option<usize>,
    ) -> BuiltEditorLayout {
        let theme = self.theme;
        let mut lines: Vec<LayoutLine> = Vec::new();
        let mut blocks: Vec<LayoutBlock> = Vec::new();
        let raw_lines: Vec<&str> = text.split('\n').collect();
        let raw_line_count = raw_lines.len();
        let mut char_offset = 0usize;
        let mut code_language = None;
        let mut code_block_start = None;
        let mut code_label = None;
        let mut code_label_width = 0.0;
        let reveal_code_block_start = reveal_line_start
            .and_then(|line_start| markdown_code_block_start_for_line_start(text, line_start));

        for (raw_idx, raw_line) in raw_lines.iter().copied().enumerate() {
            let has_newline = raw_idx + 1 < raw_line_count;
            let raw_len = raw_line.chars().count();

            // A line that is exactly `![alt](./blob/<name>?w=NNN)` becomes a
            // standalone image block: painted as a texture, reserving its
            // displayed height. Resolved against the host image registry. Only
            // in Rendered mode — Source mode shows the raw `![]()` syntax.
            if line_style == TextAreaLineStyle::Markdown
                && md_mode == MarkdownMode::Rendered
                && let Some((key, size_hint)) = parse_image_line(raw_line)
            {
                // Intrinsic aspect (height / width); None until the host provides
                // the image, when a placeholder ratio is used and a fetch is asked.
                let aspect = match self.image_size(&key) {
                    Some((iw, ih)) if iw > 0 => ih as f32 / iw as f32,
                    _ => {
                        self.request_image(&key);
                        IMAGE_PLACEHOLDER_ASPECT
                    }
                };
                let control = size_hint.map_or(SizeAxis::Height, |(axis, _)| axis);
                let (mut disp_w, mut disp_h) = match size_hint {
                    Some((SizeAxis::Height, h)) => (h / aspect.max(f32::EPSILON), h),
                    Some((SizeAxis::Width, w)) => (w, w * aspect),
                    None => (
                        DEFAULT_IMAGE_HEIGHT / aspect.max(f32::EPSILON),
                        DEFAULT_IMAGE_HEIGHT,
                    ),
                };
                // Never wider than the content column; scale height to keep aspect.
                let content_w = wrap_width.max(0.0);
                if content_w > 0.0 && disp_w > content_w {
                    disp_h *= content_w / disp_w;
                    disp_w = content_w;
                }
                push_layout_line(
                    &mut lines,
                    &mut blocks,
                    LayoutLine {
                        raw_start: char_offset,
                        raw_end: char_offset + raw_len,
                        font_size: base_font,
                        height: disp_h,
                        cum_x: vec![0.0; raw_len + 1],
                        spans: Vec::new(),
                        padding: Padding::default(),
                        image: Some(ImageLine {
                            key,
                            width: disp_w,
                            height: disp_h,
                            control,
                        }),
                    },
                    None,
                );
                char_offset += raw_len + usize::from(has_newline);
                continue;
            }

            let marker_color = theme.text_muted;
            let mut padding = Padding::default();
            let mut block = None;
            let (styled, font_size, height) = if line_style == TextAreaLineStyle::Markdown
                && let Some(fence_language) = code_fence_language(raw_line)
            {
                padding = code_block_padding();
                let closing = code_language.is_some();
                let label = if closing {
                    code_label.unwrap_or_else(|| fence_language.label())
                } else {
                    fence_language.label()
                };
                let label_width = if closing && code_label.is_some() {
                    code_label_width
                } else {
                    self.text_size(11.0, label).0
                };
                block = Some((
                    MarkdownBlockKind::Code,
                    padding,
                    code_block_bg(&theme),
                    theme.radius,
                    Some(label),
                    label_width,
                ));
                let visible = md_mode == MarkdownMode::Source
                    || reveal_line_start.is_some_and(|line_start| line_start == char_offset);
                let visible = visible
                    || reveal_code_block_start.is_some_and(|reveal_start| {
                        reveal_start == code_block_start.unwrap_or(char_offset)
                    });
                let styled = style_code_fence_line(raw_line, visible, marker_color);
                if closing {
                    code_language = None;
                    code_block_start = None;
                    code_label = None;
                    code_label_width = 0.0;
                } else {
                    code_language = Some(fence_language);
                    code_block_start = Some(char_offset);
                    code_label = Some(label);
                    code_label_width = label_width;
                }
                (styled, base_font, base_font + 4.0)
            } else if line_style == TextAreaLineStyle::Markdown
                && let Some(language) = code_language
            {
                padding = code_block_padding();
                block = Some((
                    MarkdownBlockKind::Code,
                    padding,
                    code_block_bg(&theme),
                    theme.radius,
                    code_label.or(Some(language.label())),
                    code_label_width,
                ));
                (
                    style_code_line(raw_line, language, &theme),
                    base_font,
                    base_font + 4.0,
                )
            } else {
                if line_style == TextAreaLineStyle::Markdown && is_markdown_quote_line(raw_line) {
                    padding = quote_block_padding();
                    block = Some((
                        MarkdownBlockKind::Quote,
                        padding,
                        quote_block_bg(&theme),
                        theme.radius,
                        None,
                        0.0,
                    ));
                }
                let is_focused_line = reveal_line_start.is_some_and(|start| start == char_offset);
                style_raw_line(
                    raw_line,
                    base_font,
                    line_style,
                    md_mode,
                    is_focused_line,
                    &theme,
                )
            };

            if !styled.is_empty() && styled.iter().all(|(display, _)| display.is_none()) {
                char_offset += raw_len + usize::from(has_newline);
                continue;
            }

            if styled.is_empty() {
                let line_padding = horizontal_padding(padding);
                push_layout_line(
                    &mut lines,
                    &mut blocks,
                    LayoutLine {
                        raw_start: char_offset,
                        raw_end: char_offset,
                        font_size,
                        height,
                        cum_x: vec![0.0],
                        spans: Vec::new(),
                        padding: line_padding,
                        image: None,
                    },
                    block,
                );
            } else {
                // Caret advances come from whole-run shaping (kerning, ligatures,
                // contextual forms, and grapheme clustering all included), so cum_x
                // lines up exactly with the rendered glyphs. Each glyph's advance lands
                // on the first char of its cluster; hidden markers get 0.
                let visible: String = styled.iter().filter_map(|&(disp, _)| disp).collect();
                let mut per_visible = Vec::new();
                self.char_advances(font_size, &visible, &mut per_visible);
                let mut advances = Vec::with_capacity(styled.len());
                let mut visible_idx = 0usize;
                for &(disp, _) in &styled {
                    advances.push(match disp {
                        Some(_) => {
                            let adv = per_visible.get(visible_idx).copied().unwrap_or(0.0);
                            visible_idx += 1;
                            adv
                        }
                        None => 0.0,
                    });
                }

                let mut seg_start = 0usize;
                let mut x = 0.0f32;
                let mut last_word_break = None;
                let line_wrap_width = if wrap_width > 0.0 {
                    (wrap_width - padding.horizontal()).max(0.0)
                } else {
                    0.0
                };
                for i in 0..styled.len() {
                    let adv = advances[i];
                    if line_wrap_width > 0.0 && i > seg_start && x + adv > line_wrap_width {
                        let break_at = last_word_break
                            .filter(|&break_at| break_at > seg_start && break_at <= i)
                            .unwrap_or(i);
                        let line_padding = horizontal_padding(padding);
                        push_layout_line(
                            &mut lines,
                            &mut blocks,
                            make_layout_line(
                                &styled,
                                &advances,
                                seg_start,
                                break_at,
                                char_offset,
                                font_size,
                                height,
                                line_padding,
                            ),
                            block,
                        );
                        seg_start = break_at;
                        x = advances[seg_start..i].iter().sum();
                        last_word_break = None;
                    }
                    x += adv;
                    if styled[i].0.is_some_and(char::is_whitespace) {
                        last_word_break = Some(i + 1);
                    }
                }
                let line_padding = horizontal_padding(padding);
                push_layout_line(
                    &mut lines,
                    &mut blocks,
                    make_layout_line(
                        &styled,
                        &advances,
                        seg_start,
                        styled.len(),
                        char_offset,
                        font_size,
                        height,
                        line_padding,
                    ),
                    block,
                );
            }

            char_offset += raw_len + usize::from(has_newline);
        }

        if lines.is_empty() {
            lines.push(LayoutLine {
                raw_start: 0,
                raw_end: 0,
                font_size: base_font,
                height: base_font + 4.0,
                cum_x: vec![0.0],
                spans: Vec::new(),
                image: None,
                padding: Padding::default(),
            });
        }
        BuiltEditorLayout { lines, blocks }
    }

    /// Ensure a current cached layout exists for `key`, rebuilding only on change.
    pub(super) fn ensure_editor_layout(
        &mut self,
        key: UiKey,
        text: &str,
        wrap_width: f32,
        base_font: f32,
        line_style: TextAreaLineStyle,
        md_mode: MarkdownMode,
    ) {
        let hash = u64_hash_from_string(0, text);
        let char_len = char_count(text);
        let width_key = wrap_width.max(0.0).round() as u32;
        let font_key = (base_font * 4.0).round() as u32;
        let reveal_line_start = self.markdown_reveal_line_start(key, text, line_style, md_mode);
        if let Some(layout) = self.editor_layouts.get(&key) {
            if layout.hash == hash
                && layout.char_len == char_len
                && layout.width_key == width_key
                && layout.font_key == font_key
                && layout.line_style == line_style
                && layout.md_mode == md_mode
                && layout.reveal_line_start == reveal_line_start
                && layout.images_rev == self.images_rev
            {
                return;
            }
        }
        let layout = self.build_editor_layout_revealing_line(
            text,
            wrap_width,
            base_font,
            line_style,
            md_mode,
            reveal_line_start,
        );
        // Row extents, resolved once per rebuild instead of per frame: the
        // emit window, every off-screen caret/selection/decoration y, and the
        // spacer heights are all prefix sums over these.
        let mut line_tops = Vec::with_capacity(layout.lines.len() + 1);
        let mut top = 0.0f32;
        let mut max_line_width = 0.0f32;
        for line in &layout.lines {
            line_tops.push(top);
            top += line.row_height();
            max_line_width = max_line_width.max(line.row_width());
        }
        line_tops.push(top);
        // `ensure_layout_for_box` can rebuild mid-frame (the paint and input
        // paths both call it), after the rows for this frame are already out.
        // Those rows are the previous window's, so carry it over — clamped,
        // since a rebuild can have produced fewer lines — rather than reset to
        // empty and leave `textarea_line_box` unable to name any of them.
        let line_count = layout.lines.len();
        let emitted = self
            .editor_layouts
            .get(&key)
            .map(|previous| {
                previous.emitted.start.min(line_count)..previous.emitted.end.min(line_count)
            })
            .unwrap_or(0..0);
        self.editor_layouts.insert(
            key,
            EditorLayout {
                hash,
                char_len,
                width_key,
                font_key,
                line_style,
                md_mode,
                reveal_line_start,
                images_rev: self.images_rev,
                lines: layout.lines,
                blocks: layout.blocks,
                line_tops,
                max_line_width,
                emitted,
            },
        );
    }

    fn markdown_reveal_line_start(
        &self,
        key: UiKey,
        text: &str,
        line_style: TextAreaLineStyle,
        md_mode: MarkdownMode,
    ) -> Option<usize> {
        if line_style != TextAreaLineStyle::Markdown
            || md_mode != MarkdownMode::Rendered
            || self.focus_key != Some(key)
        {
            return None;
        }
        let cursor = self
            .text_edit_states
            .get(&key)?
            .cursor
            .min(char_count(text));
        Some(raw_line_start_for_cursor(text, cursor))
    }

    /// Refresh the cached layout for an already-built editor box (used by the geometry
    /// helpers that run after `textarea_impl`). Reuses the previously recorded line
    /// style / markdown mode; rebuilds only if `text`, width or font changed.
    pub(super) fn ensure_layout_for_box(&mut self, idx: usize, text: &str) -> UiKey {
        let key = self.boxes[idx].key;
        let wrap_width = self.textarea_wrap_width(idx);
        let base_font = self.boxes[idx].style.font_size;
        let (line_style, md_mode) = self
            .editor_layouts
            .get(&key)
            .map(|l| (l.line_style, l.md_mode))
            .unwrap_or((TextAreaLineStyle::Plain, self.markdown_mode));
        self.ensure_editor_layout(key, text, wrap_width, base_font, line_style, md_mode);
        key
    }

    /// Raw-char ranges `(start, end)` for each visual line, from the cached layout.
    pub(super) fn layout_ranges(&self, key: UiKey) -> Vec<(usize, usize)> {
        self.editor_layouts
            .get(&key)
            .map(|l| {
                l.lines
                    .iter()
                    .map(|line| (line.raw_start, line.raw_end))
                    .collect()
            })
            .unwrap_or_else(|| vec![(0, 0)])
    }

    /// Pixel x (relative to the line's content origin) of a raw cursor on a visual line.
    pub(super) fn layout_caret_x(&self, key: UiKey, line_idx: usize, cursor: usize) -> f32 {
        let Some(layout) = self.editor_layouts.get(&key) else {
            return 0.0;
        };
        let Some(line) = layout.lines.get(line_idx) else {
            return 0.0;
        };
        let i = cursor
            .saturating_sub(line.raw_start)
            .min(line.cum_x.len().saturating_sub(1));
        line.cum_x[i]
    }

    /// Map a local pixel x on a visual line back to a raw cursor offset.
    pub(super) fn layout_cursor_from_x(&self, key: UiKey, line_idx: usize, x: f32) -> usize {
        let Some(layout) = self.editor_layouts.get(&key) else {
            return 0;
        };
        let Some(line) = layout.lines.get(line_idx) else {
            return 0;
        };
        // An image line is atomic: land at the start (before) or end (after)
        // depending on which half was clicked — never in the hidden markup.
        if let Some(image) = &line.image {
            return if x >= image.width * 0.5 {
                line.raw_end
            } else {
                line.raw_start
            };
        }
        for (i, &cx) in line.cum_x.iter().enumerate() {
            if cx > x {
                return line.raw_start + i.saturating_sub(1);
            }
        }
        line.raw_end
    }

    /// Displayed width of an image line, or `None` if the line isn't an image.
    pub(super) fn layout_line_image_width(&self, key: UiKey, line_idx: usize) -> Option<f32> {
        self.editor_layouts
            .get(&key)?
            .lines
            .get(line_idx)?
            .image
            .as_ref()
            .map(|image| image.width)
    }

    pub(super) fn textarea_impl<T: TextEditBuffer>(
        &mut self,
        id: &str,
        buffer: &mut T,
        options: TextAreaOptions,
        line_style: TextAreaLineStyle,
    ) -> UIBoxHandle {
        let mut flags = UIBoxFlags::MOUSE_CLICKABLE
            | UIBoxFlags::CLICK_TO_FOCUS
            | UIBoxFlags::TEXT_INPUT
            | UIBoxFlags::DRAW_BACKGROUND
            | UIBoxFlags::CLIP
            | UIBoxFlags::MULTILINE;
        if options.border {
            // The painted border blends to the accent on focus, so a chromeless editor
            // must omit the flag entirely rather than rely on a transparent color.
            flags |= UIBoxFlags::DRAW_BORDER;
        }
        if options.scroll_x {
            flags |= UIBoxFlags::SCROLL_X;
        }
        if options.scroll_y {
            flags |= UIBoxFlags::SCROLL_Y;
        }
        if !options.wrap_x {
            flags |= UIBoxFlags::NO_WRAP_X;
        }
        if line_style == TextAreaLineStyle::Markdown && self.markdown_mode == MarkdownMode::Rendered
        {
            flags |= UIBoxFlags::RICH_TEXT_HOST;
        }
        let handle = self.alloc_box(Some(id), flags);
        #[cfg(feature = "dom")]
        self.apply_pending_dom_edit(handle.key(), buffer);
        #[cfg(feature = "dom")]
        self.apply_pending_dom_focus(handle.key());
        self.boxes[handle.idx].child_layout_axis = Axis::Y;
        self.boxes[handle.idx].pref_size = [UISize::ParentPct(1.0), UISize::ParentPct(1.0)];
        self.boxes[handle.idx].padding = Padding::all(10.0);
        // Text inset comes from padding; the generic 2px draw margin would otherwise
        // double the inset and push glyphs away from the padding box edge.
        self.boxes[handle.idx].style.margin = 0.0;
        self.boxes[handle.idx].style.bg_color = self.theme.input_bg;
        self.boxes[handle.idx].style.border_color = self.theme.border;
        self.boxes[handle.idx].style.corner_radius = self.theme.radius;
        self.boxes[handle.idx].child_gap = 2.0;
        if let Some(font_size) = options.font_size {
            self.boxes[handle.idx].style.font_size = font_size;
        }
        if let Some(padding) = options.padding {
            self.boxes[handle.idx].padding = padding;
        }
        self.apply_click_to_focus(handle);
        let mut text = buffer.text();
        #[cfg(feature = "dom")]
        if flags.contains(UIBoxFlags::RICH_TEXT_HOST) {
            self.apply_pending_dom_selection(handle.key(), &text);
        }

        let content_width = if options.wrap_x {
            (self.boxes[handle.idx].rect.x1
                - self.boxes[handle.idx].rect.x0
                - self.boxes[handle.idx].padding.horizontal()
                - self.boxes[handle.idx].style.margin * 2.0)
                .max(0.0)
        } else {
            0.0
        };
        let base_font = self.boxes[handle.idx].style.font_size;
        let md_mode = self.markdown_mode;
        let key = handle.key;

        // Resolve the cached layout before handling input so vertical caret motion sees
        // the current text, then refresh after any edit before emitting the line boxes.
        // Both calls are cache hits (no parsing/wrapping) when the text is unchanged.
        self.ensure_editor_layout(key, &text, content_width, base_font, line_style, md_mode);
        if self.box_is_focused(handle) {
            self.apply_text_input(
                handle,
                buffer,
                &mut text,
                true,
                options.read_only,
                line_style,
            );
            if options.border {
                self.boxes[handle.idx].style.border_color = self.theme.accent;
            }
            // Inject the IME preedit (composing text) into the displayed string at the
            // caret. It is display-only: the synced buffer is untouched until the IME
            // commits (which arrives as ordinary character events).
            if let Some(preedit) = self.ime_preedit.clone()
                && !preedit.is_empty()
            {
                let caret = self.text_cursor(key);
                let caret_byte = char_to_byte(&text, caret);
                text.insert_str(caret_byte, &preedit);
            }
        }
        self.boxes[handle.idx].string = Some(text.clone());
        self.ensure_editor_layout(key, &text, content_width, base_font, line_style, md_mode);

        self.parent_stack.push(handle.idx);
        // Take ownership of the cached layout while emitting boxes to avoid cloning the
        // (potentially large) line/span data every frame, then return it to the cache.
        let mut layout = self
            .editor_layouts
            .remove(&key)
            .expect("layout ensured above");
        // Only the lines near the viewport become boxes; the rest are stood in
        // for by a spacer of exactly their combined height, so the emitted rows
        // land on the same y they always did and `total_children_size` — and
        // with it `scroll_max` and the scrollbar thumb — is unchanged. Without
        // this a 4k-line note rebuilt ~16k boxes (and ~5 String allocations
        // each) every frame, which is linear in the *document*, not in what is
        // actually on screen.
        let window = self.visible_line_window(handle.idx, &layout);
        let gap = self.boxes[handle.idx].child_gap;
        let line_count = layout.lines.len();
        // Only a horizontally scrolling text area needs the spacers to carry a
        // width (see `emit_line_spacer`). In wrap mode `SCROLL_X` is off, which
        // means `reconcile_overflow` is free to shrink children to fit — so a
        // spacer as wide as the widest line could squeeze the real rows.
        let spacer_width = if self.boxes[handle.idx].flags.scrolls_x() {
            layout.max_line_width
        } else {
            0.0
        };
        if window.start > 0 {
            // The first emitted row sits one `child_gap` after this spacer, so
            // the spacer is that much shorter than the skipped lines' extent.
            let height = layout.line_top(window.start, gap) - gap;
            self.emit_line_spacer("###textarea_lead_spacer", height, spacer_width);
        }
        for line_idx in window.clone() {
            self.emit_layout_line(&layout.lines[line_idx], line_idx);
        }
        if window.end < line_count {
            let height = layout.line_top(line_count, gap) - gap - layout.line_top(window.end, gap);
            self.emit_line_spacer("###textarea_trail_spacer", height, spacer_width);
        }
        layout.emitted = window;
        self.editor_layouts.insert(key, layout);
        self.parent_stack.pop();
        // Click/drag selection is resolved post-layout (apply_textarea_mouse_selections)
        // so it sees the final padding/rect, which the caller may set after this returns.
        handle
    }

    /// The visual lines worth turning into boxes this frame: everything on
    /// screen plus a viewport of overscan either side, unioned over the
    /// current *and* the target scroll offset so a smooth-scroll glide — or a
    /// caret-follow jump the previous frame queued onto `scroll_target` — is
    /// already covered by the time it becomes visible. Both are read after
    /// `alloc_box` has applied this frame's wheel signal, so a scroll and the
    /// window that serves it land in the same frame.
    fn visible_line_window(&self, idx: usize, layout: &EditorLayout) -> Range<usize> {
        let line_count = layout.lines.len();
        // The DOM rich-text host mounts these rows as the contenteditable's
        // own content, so a line that isn't emitted isn't merely unpainted —
        // it isn't in the document, and a selection (Ctrl+A most obviously)
        // could not reach past the window. Native first; virtualizing the DOM
        // backend needs its own selection story.
        #[cfg(feature = "dom")]
        if self.dom.is_some() {
            return 0..line_count;
        }
        // Nothing to virtualize against without a viewport to clip to.
        if !self.boxes[idx].flags.scrolls_y() {
            return 0..line_count;
        }
        let gap = self.boxes[idx].child_gap;
        // A text area that has never been laid out has a zero rect, and an
        // empty window would flash the note blank for its first frame. The
        // window is only ever a lower bound on what to emit, so falling back
        // to the whole surface just over-emits once.
        let viewport = {
            let height = self.boxes[idx].rect.height() - self.boxes[idx].padding.vertical();
            if height > 1.0 {
                height
            } else {
                self.size.height
            }
        };
        let scroll = self.boxes[idx].scroll.y;
        let target = self.boxes[idx].scroll_target.y;
        let first = layout.line_at_offset(scroll.min(target) - viewport, gap);
        let last = layout.line_at_offset(scroll.max(target) + viewport * 2.0, gap);
        first..(last + 1).min(line_count)
    }

    /// A stand-in for the visual lines `visible_line_window` skipped: no
    /// content, exactly their combined height, and — when the text area scrolls
    /// horizontally — the document's widest line for a width, so that extent
    /// stays the whole note's rather than just the rows on screen.
    fn emit_line_spacer(&mut self, id: &str, height: f32, width: f32) {
        let spacer = self.alloc_box(Some(id), UIBoxFlags::NONE);
        self.boxes[spacer.idx].pref_size = [
            UISize::Pixels(width.max(0.0)),
            UISize::Pixels(height.max(0.0)),
        ];
        self.boxes[spacer.idx].style.margin = 0.0;
    }

    /// The row box emitted for visual line `line_idx`, or `None` when that
    /// line is outside the window this frame emitted. A line index is *not* a
    /// child index: the lead spacer, when there is one, takes slot 0.
    pub(super) fn textarea_line_box(&self, idx: usize, line_idx: usize) -> Option<usize> {
        let layout = self.editor_layouts.get(&self.boxes[idx].key)?;
        if !layout.emitted.contains(&line_idx) {
            return None;
        }
        let lead = usize::from(layout.emitted.start > 0);
        let child_pos = line_idx - layout.emitted.start + lead;
        self.boxes[idx].children.get(child_pos).copied()
    }

    /// Screen-space geometry of visual line `line_idx`'s row, taken from the
    /// cached layout rather than from its box. Answers for every line, not
    /// just the emitted ones — an off-screen caret, a selection running past
    /// the viewport and a block decoration straddling it all need a real y,
    /// and the uniform-line-height guesses this replaces were only ever right
    /// for a plain-text note.
    pub(super) fn textarea_line_rect(&self, idx: usize, line_idx: usize) -> Option<LineRect> {
        let layout = self.editor_layouts.get(&self.boxes[idx].key)?;
        let line = layout.lines.get(line_idx)?;
        // Children are positioned at `rect.y0 + padding.top - scroll` plus the
        // preceding rows' extent; the box's own margin is not part of that
        // (see `position_children_on_main_axis`).
        let y0 = self.boxes[idx].rect.y0 + self.boxes[idx].padding.top - self.boxes[idx].scroll.y
            + layout.line_top(line_idx, self.boxes[idx].child_gap);
        Some(LineRect {
            y0,
            y1: y0 + line.row_height(),
            padding: line.padding,
            font_size: line.font_size,
        })
    }

    /// Emit one visual line as a horizontal row of pre-styled text segments. Horizontal
    /// caret/selection geometry comes from the cached `cum_x`, vertical from
    /// `textarea_line_rect` — neither reads the row box back, so a line stays
    /// addressable while it is scrolled out of the emitted window.
    pub(super) fn emit_layout_line(&mut self, line: &LayoutLine, idx: usize) {
        if let Some(image) = &line.image {
            let row_id = format!("###textarea_line_{idx}");
            let row = self.alloc_box(Some(&row_id), UIBoxFlags::NONE);
            self.boxes[row.idx].child_layout_axis = Axis::X;
            self.boxes[row.idx].pref_size = [
                UISize::ChildrenSum,
                UISize::Pixels(image.height + line.padding.vertical()),
            ];
            self.boxes[row.idx].padding = line.padding;
            self.boxes[row.idx].style.margin = 0.0;
            self.boxes[row.idx].richtext_span = Some((line.raw_start, line.raw_end));

            self.parent_stack.push(row.idx);
            let img_id = format!("{}###textarea_img_{idx}", image.key);
            let img_box = self.alloc_box(
                Some(&img_id),
                UIBoxFlags::DRAW_IMAGE | UIBoxFlags::MOUSE_CLICKABLE,
            );
            self.set_display_string(img_box.idx, image.key.clone());
            self.boxes[img_box.idx].pref_size =
                [UISize::Pixels(image.width), UISize::Pixels(image.height)];
            self.boxes[img_box.idx].style.margin = 0.0;
            self.boxes[img_box.idx].richtext_span = Some((line.raw_start, line.raw_end));
            // The `MOUSE_CLICKABLE` flag marks this as a resizable inline image
            // (the viewer uses a plain image box). The resize drag itself is
            // driven by the textarea's own mouse handling — a nested clickable
            // never receives events, since the parent textarea consumes them.
            self.parent_stack.pop();
            return;
        }
        // Visible spans only: a hidden span's `text` is zero-width
        // placeholder filler standing in for markdown markers the reader
        // never sees (see `LayoutSpan`), so including it here would put
        // U+200Bs into this row's display string — its `data-mae-id` on the
        // DOM backend, and the text a testkit snapshot reports for it.
        let display: String = line
            .spans
            .iter()
            .filter(|s| !s.hidden)
            .map(|s| s.text.as_str())
            .collect();
        let line_id = format!("{display}###textarea_line_{idx}");
        let row = self.alloc_box(Some(&line_id), UIBoxFlags::NONE);
        self.boxes[row.idx].child_layout_axis = Axis::X;
        self.boxes[row.idx].pref_size = [
            UISize::ChildrenSum,
            UISize::Pixels(line.height + line.padding.vertical()),
        ];
        self.boxes[row.idx].padding = line.padding;
        self.boxes[row.idx].child_gap = 0.0;
        self.boxes[row.idx].style.margin = 0.0;
        self.boxes[row.idx].style.font_size = line.font_size;
        self.boxes[row.idx].richtext_span = Some((line.raw_start, line.raw_end));
        self.set_display_string(row.idx, display);

        self.parent_stack.push(row.idx);
        if line.spans.is_empty() {
            // Keep the line's height even when there is nothing to draw. Also
            // the only DOM node a rich-text host can land a caret on for a
            // wholly-empty line (no span children to serve as a text anchor)
            // — see `paint_dom.rs`'s richtext caret placement.
            let spacer = self.alloc_box(None, UIBoxFlags::NONE);
            self.boxes[spacer.idx].pref_size = [UISize::Pixels(0.0), UISize::Pixels(line.height)];
            self.boxes[spacer.idx].style.margin = 0.0;
            self.boxes[spacer.idx].richtext_span = Some((line.raw_start, line.raw_end));
        } else {
            for (span_idx, span) in line.spans.iter().enumerate() {
                // A hidden span (see `LayoutSpan`'s doc comment) is a
                // navigability anchor for the DOM rich-text host only —
                // native never draws hidden markers and doesn't need a box
                // for them at all, so skip it there for the exact same
                // (zero) box count as before this mechanism existed.
                if span.hidden {
                    #[cfg(feature = "dom")]
                    let dom_backend = self.dom.is_some();
                    #[cfg(not(feature = "dom"))]
                    let dom_backend = false;
                    if !dom_backend {
                        continue;
                    }
                }
                let seg_id = format!("{}###textarea_seg_{idx}_{span_idx}", span.text);
                let seg = self.alloc_box(Some(&seg_id), UIBoxFlags::DRAW_TEXT);
                self.set_display_string(seg.idx, span.text.clone());
                self.boxes[seg.idx].pref_size =
                    [UISize::TextContent(0.0), UISize::TextContent(0.0)];
                // Zero margin so glyph x positions match the cached cum_x exactly.
                self.boxes[seg.idx].style.margin = 0.0;
                // font-size 0, not the line's real size, for a hidden span:
                // real (non-zero) DOM text so the browser can still place a
                // caret at any raw offset inside it, but rendered at zero
                // size so it stays invisible — see `LayoutSpan`'s doc comment.
                self.boxes[seg.idx].style.font_size =
                    if span.hidden { 0.0 } else { line.font_size };
                self.boxes[seg.idx].style.text_color = span.color;
                self.boxes[seg.idx].richtext_span =
                    Some((span.raw_start, span.raw_start + span.text.chars().count()));
            }
        }
        self.parent_stack.pop();
    }

    pub(super) fn visual_line_col_from_cursor_with_ranges(
        &self,
        ranges: &[(usize, usize)],
        cursor: usize,
    ) -> (usize, usize) {
        for (line_idx, &(start, end)) in ranges.iter().enumerate() {
            if start == end && cursor == start {
                return (line_idx, 0);
            }
            if cursor >= start && cursor < end {
                return (line_idx, cursor - start);
            }
            if cursor == end {
                return (line_idx, end - start);
            }
        }
        if let Some(&(start, end)) = ranges.last() {
            let col = cursor.saturating_sub(start);
            if end > start {
                (ranges.len() - 1, col.min(end - start))
            } else {
                (ranges.len() - 1, 0)
            }
        } else {
            (0, 0)
        }
    }

    pub(super) fn cursor_from_visual_line_col_with_ranges(
        &self,
        ranges: &[(usize, usize)],
        visual_line: usize,
        col: usize,
    ) -> usize {
        if visual_line >= ranges.len() {
            ranges.last().map(|&(_, end)| end).unwrap_or(0)
        } else {
            let (start, end) = ranges[visual_line];
            (start + col).min(end)
        }
    }

    /// Copy `text` to the clipboard. Mirrors it to the OS clipboard when running with a
    /// real window (`drawer` present) so it interoperates with other apps; the in-app
    /// copy is kept as a fallback for headless/test runs and platforms without OS
    /// clipboard integration.
    pub(super) fn write_clipboard(&mut self, text: String) {
        if self.drawer.is_some() {
            os::clipboard_set(&text);
        }
        self.clipboard = text;
    }

    /// Read the clipboard, preferring the OS clipboard (so externally-copied text can be
    /// pasted) and falling back to the in-app copy when no system string is available.
    pub(super) fn read_clipboard(&self) -> String {
        if self.drawer.is_some() {
            if let Some(text) = os::clipboard_get() {
                return text;
            }
        }
        self.clipboard.clone()
    }

    /// Record the pre-edit state so a following mutation can be undone. Consecutive edits
    /// of the same coalescing kind (a typing run, or a backspace run) reuse the existing
    /// entry instead of pushing a new one, so undo works at word granularity rather than
    /// per keystroke. Always invalidates the redo stack — a fresh edit forks history.
    pub(super) fn record_undo_before_edit(
        &mut self,
        key: UiKey,
        kind: EditKind,
        text: &str,
        has_selection: bool,
    ) {
        let cursor = self.text_cursor(key);
        let history = self.undo_states.entry(key).or_default();
        let continues_run = matches!(kind, EditKind::Insert | EditKind::InsertBreak)
            && history.coalescing == Some(EditKind::Insert)
            || kind == EditKind::Delete && history.coalescing == Some(EditKind::Delete);
        let coalesce = continues_run && !has_selection;
        if !coalesce {
            history.undo.push(UndoSnapshot {
                text: text.to_string(),
                cursor,
            });
            if history.undo.len() > UNDO_HISTORY_LIMIT {
                history.undo.remove(0);
            }
        }
        history.redo.clear();
        // A whitespace insert closes the current word so the next word is its own step.
        history.coalescing = match kind {
            EditKind::Insert => Some(EditKind::Insert),
            EditKind::Delete => Some(EditKind::Delete),
            EditKind::InsertBreak | EditKind::Boundary => None,
        };
    }

    /// Break any in-progress coalescing run so the next edit starts a new undo step.
    /// Called on cursor moves and after an undo/redo.
    pub(super) fn break_undo_coalescing(&mut self, key: UiKey) {
        if let Some(history) = self.undo_states.get_mut(&key) {
            history.coalescing = None;
        }
    }

    pub(super) fn undo_text_edit<T: TextEditBuffer>(
        &mut self,
        key: UiKey,
        buffer: &mut T,
        text: &mut String,
    ) -> bool {
        let Some(target) = self
            .undo_states
            .get_mut(&key)
            .and_then(|history| history.undo.pop())
        else {
            return false;
        };
        let current = UndoSnapshot {
            text: text.clone(),
            cursor: self.text_cursor(key),
        };
        Self::apply_text_via_diff(buffer, text, &target.text);
        self.restore_history_caret(key, target.cursor);
        if let Some(history) = self.undo_states.get_mut(&key) {
            history.redo.push(current);
            history.coalescing = None;
        }
        true
    }

    pub(super) fn redo_text_edit<T: TextEditBuffer>(
        &mut self,
        key: UiKey,
        buffer: &mut T,
        text: &mut String,
    ) -> bool {
        let Some(target) = self
            .undo_states
            .get_mut(&key)
            .and_then(|history| history.redo.pop())
        else {
            return false;
        };
        let current = UndoSnapshot {
            text: text.clone(),
            cursor: self.text_cursor(key),
        };
        Self::apply_text_via_diff(buffer, text, &target.text);
        self.restore_history_caret(key, target.cursor);
        if let Some(history) = self.undo_states.get_mut(&key) {
            history.undo.push(current);
            history.coalescing = None;
        }
        true
    }

    fn restore_history_caret(&mut self, key: UiKey, cursor: usize) {
        let state = self.text_edit_states.entry(key).or_default();
        state.cursor = cursor;
        state.selection = None;
        state.desired_column = None;
        self.reset_caret_blink(key);
    }

    /// Reconcile `current` (and the backing `buffer`) to `target` with the minimal
    /// single-span splice: keep the common prefix/suffix and replace only the differing
    /// middle. Going through the buffer's insert/delete keeps a CRDT-backed store (and
    /// its sync) consistent, rather than swapping the whole document.
    fn apply_text_via_diff<T: TextEditBuffer>(buffer: &mut T, current: &mut String, target: &str) {
        if current.as_str() == target {
            return;
        }
        let cur: Vec<char> = current.chars().collect();
        let tgt: Vec<char> = target.chars().collect();
        let mut prefix = 0;
        while prefix < cur.len() && prefix < tgt.len() && cur[prefix] == tgt[prefix] {
            prefix += 1;
        }
        let mut suffix = 0;
        while suffix < cur.len() - prefix
            && suffix < tgt.len() - prefix
            && cur[cur.len() - 1 - suffix] == tgt[tgt.len() - 1 - suffix]
        {
            suffix += 1;
        }
        let delete_range = (prefix, cur.len() - suffix);
        if delete_range.1 > delete_range.0 {
            Self::apply_delete_range(buffer, current, delete_range);
        }
        let insert: String = tgt[prefix..tgt.len() - suffix].iter().collect();
        if !insert.is_empty() {
            buffer.insert_text(prefix, &insert);
            let byte = char_to_byte(current, prefix);
            current.insert_str(byte, &insert);
        }
    }

    pub(super) fn apply_text_input<T: TextEditBuffer>(
        &mut self,
        handle: UIBoxHandle,
        buffer: &mut T,
        text: &mut String,
        multiline: bool,
        read_only: bool,
        line_style: TextAreaLineStyle,
    ) {
        let key = handle.key;
        self.ensure_text_state(key, text);
        let mut ev_idx = 0;
        while ev_idx < self.events.len() {
            let ev = self.events[ev_idx];
            if ev.ty != OSEventType::Press {
                ev_idx += 1;
                continue;
            }
            let taken =
                self.apply_text_event(key, buffer, text, multiline, read_only, line_style, ev);
            if taken {
                // A consumed keystroke ends any in-progress click streak: a
                // later click at the same spot must start a fresh single
                // click, not be coalesced into a double/triple-click (which
                // would select and overwrite a word/paragraph).
                self.text_click_streak = TextClickStreak::default();
                self.remove_event(ev_idx);
            } else {
                ev_idx += 1;
            }
        }
    }

    pub(super) fn ensure_text_state(&mut self, key: UiKey, buffer: &str) {
        let len = char_count(buffer);
        let state = self.text_edit_states.entry(key).or_default();
        state.cursor = state.cursor.min(len);
        if let Some(selection) = state.selection.as_mut() {
            selection.anchor = selection.anchor.min(len);
            selection.cursor = selection.cursor.min(len);
            if selection.anchor == selection.cursor {
                state.selection = None;
            }
        }
    }

    pub(super) fn apply_text_event<T: TextEditBuffer>(
        &mut self,
        key: UiKey,
        buffer: &mut T,
        text: &mut String,
        multiline: bool,
        read_only: bool,
        line_style: TextAreaLineStyle,
        ev: OSEvent,
    ) -> bool {
        let OSKey::Keyboard(key_code) = ev.key else {
            return false;
        };
        let shift = has_flag(ev.flags, OSEventFlag::Shift);
        let primary = primary_modifier(ev.flags);

        if primary {
            match key_code {
                OSKeyCode::KeyA => {
                    let len = char_count(text);
                    let state = self.text_edit_states.entry(key).or_default();
                    state.cursor = len;
                    state.selection = Some(TextSelection {
                        anchor: 0,
                        cursor: len,
                    });
                    self.reset_caret_blink(key);
                    return true;
                }
                OSKeyCode::KeyC => {
                    if let Some(selected) = selected_text(
                        text,
                        self.text_edit_states
                            .get(&key)
                            .and_then(TextEditState::selection_range),
                    ) {
                        self.write_clipboard(selected);
                    }
                    return true;
                }
                OSKeyCode::KeyX => {
                    let range = self
                        .text_edit_states
                        .get(&key)
                        .and_then(TextEditState::selection_range);
                    if let Some(selected) = selected_text(text, range) {
                        self.write_clipboard(selected);
                        if !read_only {
                            self.record_undo_before_edit(key, EditKind::Boundary, text, true);
                            Self::apply_delete_range(buffer, text, range.unwrap());
                            let state = self.text_edit_states.entry(key).or_default();
                            state.cursor = range.unwrap().0;
                            state.clear_selection();
                        }
                    }
                    self.reset_caret_blink(key);
                    return true;
                }
                OSKeyCode::KeyV => {
                    if read_only {
                        return true;
                    }
                    // In multiline editors, an image on the clipboard is routed
                    // to the host (which uploads it + inserts a `./blob/<name>`
                    // link) instead of pasting text. Gated on a real window
                    // (`drawer`) so headless/test runs don't touch the OS
                    // clipboard, matching `write_clipboard`/`read_clipboard`.
                    if multiline
                        && self.drawer.is_some()
                        && self.pasted_image.is_none()
                        && let Some(bytes) = os::clipboard_get_image()
                    {
                        self.pasted_image = Some(bytes);
                        self.reset_caret_blink(key);
                        return true;
                    }
                    let clipboard = self.read_clipboard();
                    self.record_undo_before_edit(key, EditKind::Boundary, text, false);
                    self.replace_selection_or_insert(key, buffer, text, &clipboard);
                    self.reset_caret_blink(key);
                    return true;
                }
                OSKeyCode::KeyZ => {
                    if !read_only {
                        if shift {
                            self.redo_text_edit(key, buffer, text);
                        } else {
                            self.undo_text_edit(key, buffer, text);
                        }
                    }
                    return true;
                }
                _ => {}
            }
        }

        match key_code {
            OSKeyCode::KeyBackspace => {
                if read_only {
                    return true;
                }
                let has_selection = self.has_text_selection(key);
                if has_selection || self.text_cursor(key) > 0 {
                    self.record_undo_before_edit(key, EditKind::Delete, text, has_selection);
                }
                if !self.delete_selection(key, buffer, text) {
                    let state = self.text_edit_states.entry(key).or_default();
                    if state.cursor > 0 {
                        let pos = state.cursor;
                        // Delete the whole preceding grapheme cluster, not one char.
                        let prev = cursor_left(text, pos);
                        Self::apply_delete_range(buffer, text, (prev, pos));
                        state.cursor = prev;
                    }
                }
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyDelete => {
                if read_only {
                    return true;
                }
                let has_selection = self.has_text_selection(key);
                if has_selection || self.text_cursor(key) < char_count(text) {
                    self.record_undo_before_edit(key, EditKind::Delete, text, has_selection);
                }
                if !self.delete_selection(key, buffer, text) {
                    let state = self.text_edit_states.entry(key).or_default();
                    let len = char_count(text);
                    if state.cursor < len {
                        let pos = state.cursor;
                        // Forward-delete the whole next grapheme cluster.
                        let next = cursor_right(text, pos);
                        Self::apply_delete_range(buffer, text, (pos, next));
                    }
                }
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyEnter if multiline => {
                if read_only {
                    return true;
                }
                let range = self
                    .text_edit_states
                    .get(&key)
                    .and_then(TextEditState::selection_range);
                let cursor = self.text_cursor(key);
                if shift
                    && line_style == TextAreaLineStyle::Markdown
                    && range.is_none()
                    && self.exit_markdown_code_block(key, buffer, text, false)
                {
                    self.reset_caret_blink(key);
                    return true;
                }
                self.record_undo_before_edit(key, EditKind::Boundary, text, false);
                if line_style == TextAreaLineStyle::Markdown
                    && range.is_none()
                    && let Some((insert, caret_delta)) =
                        markdown_code_fence_enter_insert(text, cursor)
                {
                    self.replace_selection_or_insert(key, buffer, text, &insert);
                    let state = self.text_edit_states.entry(key).or_default();
                    state.cursor = cursor + caret_delta;
                    state.desired_column = None;
                } else {
                    self.replace_selection_or_insert(key, buffer, text, "\n");
                }
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyEscape => {
                self.focus_key = None;
                true
            }
            OSKeyCode::KeyLeftArrow => {
                self.move_text_cursor(key, text, cursor_left(text, self.text_cursor(key)), shift);
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyRightArrow => {
                self.move_text_cursor(key, text, cursor_right(text, self.text_cursor(key)), shift);
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyHome => {
                self.move_text_cursor(key, text, line_home(text, self.text_cursor(key)), shift);
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyEnd => {
                self.move_text_cursor(key, text, line_end(text, self.text_cursor(key)), shift);
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyUpArrow if multiline => {
                self.move_vertical(key, text, -1, shift);
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyDownArrow if multiline => {
                if line_style == TextAreaLineStyle::Markdown
                    && !read_only
                    && !self.has_text_selection(key)
                    && self.exit_markdown_code_block(key, buffer, text, shift)
                {
                    self.reset_caret_blink(key);
                    return true;
                }
                self.move_vertical(key, text, 1, shift);
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyPageUp => {
                if multiline {
                    self.move_page(key, text, -1, shift);
                } else {
                    self.move_text_cursor(key, text, 0, shift);
                }
                self.reset_caret_blink(key);
                true
            }
            OSKeyCode::KeyPageDown => {
                if multiline {
                    self.move_page(key, text, 1, shift);
                } else {
                    self.move_text_cursor(key, text, char_count(text), shift);
                }
                self.reset_caret_blink(key);
                true
            }
            _ => {
                if let Some(c) = ev.chars {
                    if !c.is_ascii_control() {
                        if read_only {
                            return true;
                        }
                        // Whitespace closes the current word so undo steps word-by-word.
                        let kind = if c.is_whitespace() {
                            EditKind::InsertBreak
                        } else {
                            EditKind::Insert
                        };
                        self.record_undo_before_edit(key, kind, text, self.has_text_selection(key));
                        let mut s = String::new();
                        s.push(c);
                        self.replace_selection_or_insert(key, buffer, text, &s);
                        self.reset_caret_blink(key);
                        return true;
                    }
                }
                false
            }
        }
    }

    fn has_text_selection(&self, key: UiKey) -> bool {
        self.text_edit_states
            .get(&key)
            .and_then(TextEditState::selection_range)
            .is_some()
    }

    pub(super) fn text_cursor(&self, key: UiKey) -> usize {
        self.text_edit_states
            .get(&key)
            .map(|state| state.cursor)
            .unwrap_or(0)
    }

    pub(super) fn reset_caret_blink(&mut self, key: UiKey) {
        let now = self.now_seconds();
        let state = self.text_edit_states.entry(key).or_default();
        state.last_interaction_time = now;
    }

    pub(super) fn set_text_cursor(&mut self, key: UiKey, cursor: usize, extend_selection: bool) {
        // Moving the caret (arrow keys, clicks, drags) ends any typing/deletion run so the
        // next edit starts a fresh undo step.
        self.break_undo_coalescing(key);
        let state = self.text_edit_states.entry(key).or_default();
        if extend_selection {
            let anchor = state.selection.map(|s| s.anchor).unwrap_or(state.cursor);
            state.selection = Some(TextSelection { anchor, cursor });
        } else {
            state.selection = None;
        }
        state.cursor = cursor;
        state.desired_column = None;
    }

    pub(super) fn move_text_cursor(
        &mut self,
        key: UiKey,
        buffer: &str,
        cursor: usize,
        extend_selection: bool,
    ) {
        self.set_text_cursor(key, cursor.min(char_count(buffer)), extend_selection);
    }

    pub(super) fn move_vertical(
        &mut self,
        key: UiKey,
        buffer: &str,
        delta: isize,
        extend_selection: bool,
    ) {
        self.move_visual_lines(key, buffer, delta, extend_selection);
    }

    pub(super) fn move_page(
        &mut self,
        key: UiKey,
        buffer: &str,
        direction: isize,
        extend_selection: bool,
    ) {
        let Some(idx) = self.box_from_key(key) else {
            return;
        };
        let visible_height = self.textarea_visible_height(idx);
        if self.boxes[idx].flags.scrolls_y() {
            let delta = visible_height * direction.signum() as f32;
            self.boxes[idx].scroll_target.y = (self.boxes[idx].scroll_target.y + delta).max(0.0);
            self.request_repaint();
        }
        let lines = (visible_height / self.textarea_line_height(idx, 0).max(1.0))
            .floor()
            .max(1.0) as isize;
        self.move_visual_lines(key, buffer, direction.signum() * lines, extend_selection);
    }

    pub(super) fn move_visual_lines(
        &mut self,
        key: UiKey,
        buffer: &str,
        delta: isize,
        extend_selection: bool,
    ) {
        let cursor = self.text_cursor(key);
        let Some(idx) = self.box_from_key(key) else {
            return;
        };
        self.ensure_layout_for_box(idx, buffer);
        let ranges = self.layout_ranges(key);
        let (visual_line, col) = self.visual_line_col_from_cursor_with_ranges(&ranges, cursor);
        let state = self.text_edit_states.entry(key).or_default();
        let desired_col = state.desired_column.unwrap_or(col);
        state.desired_column = Some(desired_col);
        let line_count = ranges.len().max(1);
        let next_line = (visual_line as isize + delta).clamp(0, line_count as isize - 1) as usize;
        let next_cursor = if self.layout_line_image_width(key, next_line).is_some() {
            // Landing on an image via up/down goes to its beginning (atomic).
            ranges[next_line].0
        } else {
            self.cursor_from_visual_line_col_with_ranges(&ranges, next_line, desired_col)
        };
        self.move_text_cursor(key, buffer, next_cursor, extend_selection);
        if let Some(state) = self.text_edit_states.get_mut(&key) {
            state.desired_column = Some(desired_col);
        }
    }

    pub(super) fn textarea_visible_height(&self, idx: usize) -> f32 {
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        (rect.height() - padding.vertical()).max(1.0)
    }

    pub(super) fn textarea_line_height(&self, idx: usize, line_idx: usize) -> f32 {
        // The row box's `Pixels(...)` height *is* `row_height()`, and the
        // layout has it for off-screen lines too.
        self.editor_layouts
            .get(&self.boxes[idx].key)
            .and_then(|layout| layout.lines.get(line_idx))
            .map(LayoutLine::row_height)
            .unwrap_or(self.theme.size_text + 6.0)
    }

    pub(super) fn replace_selection_or_insert<T: TextEditBuffer>(
        &mut self,
        key: UiKey,
        buffer: &mut T,
        current_text: &mut String,
        insert_text: &str,
    ) {
        self.delete_selection(key, buffer, current_text);
        let state = self.text_edit_states.entry(key).or_default();
        let cursor = state.cursor.min(char_count(current_text));
        buffer.insert_text(cursor, insert_text);
        let byte = char_to_byte(current_text, cursor);
        current_text.insert_str(byte, insert_text);
        state.cursor = cursor + char_count(insert_text);
        state.clear_selection();
        state.desired_column = None;
    }

    pub(super) fn exit_markdown_code_block<T: TextEditBuffer>(
        &mut self,
        key: UiKey,
        buffer: &mut T,
        current_text: &mut String,
        extend_selection: bool,
    ) -> bool {
        let cursor = self.text_cursor(key);
        let Some(exit) = markdown_exit_code_block_after_current_line(current_text, cursor) else {
            return false;
        };

        if let Some(pos) = exit.insert_newline_at {
            self.record_undo_before_edit(key, EditKind::Boundary, current_text, false);
            buffer.insert_text(pos, "\n");
            let byte = char_to_byte(current_text, pos);
            current_text.insert_str(byte, "\n");
        }

        self.move_text_cursor(
            key,
            current_text,
            exit.cursor.min(char_count(current_text)),
            extend_selection,
        );
        true
    }

    pub(super) fn delete_selection<T: TextEditBuffer>(
        &mut self,
        key: UiKey,
        buffer: &mut T,
        current_text: &mut String,
    ) -> bool {
        let range = self
            .text_edit_states
            .get(&key)
            .and_then(TextEditState::selection_range);
        let Some(range) = range else {
            return false;
        };
        Self::apply_delete_range(buffer, current_text, range);
        let state = self.text_edit_states.entry(key).or_default();
        state.cursor = range.0;
        state.clear_selection();
        state.desired_column = None;
        true
    }

    pub(super) fn apply_delete_range<T: TextEditBuffer>(
        buffer: &mut T,
        current_text: &mut String,
        range: (usize, usize),
    ) {
        buffer.delete_range(range);
        delete_char_range(current_text, range);
    }

    pub(super) fn apply_line_edit_mouse_selection(&mut self, handle: UIBoxHandle, buffer: &str) {
        if handle.pressed() {
            let cursor =
                self.cursor_from_line_edit_point(handle, buffer, handle.signal.left_press_pos);
            let click_count = self
                .register_text_click(handle.key, handle.signal.left_press_pos.unwrap_or_default());
            self.apply_text_click_selection(handle.key, buffer, cursor, click_count);
            self.reset_caret_blink(handle.key);
        }
        if handle.dragging() {
            let cursor = self.cursor_from_line_edit_point(handle, buffer, self.mouse);
            self.set_text_cursor(handle.key, cursor, true);
            self.reset_caret_blink(handle.key);
        }
    }

    /// Map textarea press/drag to a caret position. Runs after layout (see
    /// [`Self::apply_textarea_mouse_selections`]) so it sees the final box rect and
    /// padding — the caller may set padding after building the widget, and the
    /// hit-test must agree with where the glyphs are actually drawn.
    fn apply_textarea_mouse_selection_for(&mut self, idx: usize) {
        let key = self.boxes[idx].key;
        let signal = self.boxes[idx].signal;
        let pressed = signal.pressed();
        let dragging = signal.dragging();
        // An in-progress image resize is kept alive for as long as the mouse
        // button is held — including the gap between the initial press and the
        // first movement past the drag threshold, where the box reports neither
        // `pressed` nor `dragging`. It ends only on release.
        if self.image_drag.is_some() {
            if self.left_mouse_down {
                self.drive_image_resize();
                return;
            }
            self.image_drag = None;
        }
        if !pressed && !dragging {
            return;
        }
        // Inline-image resize takes priority over text selection: begin on a
        // press that lands in an image's grip.
        if pressed && let Some(drag) = self.image_resize_grip_at(idx, signal.left_press_pos) {
            self.image_drag = Some(drag);
            self.drive_image_resize();
            return;
        }
        let buffer = self.boxes[idx].string.clone().unwrap_or_default();
        if pressed {
            let cursor = self.cursor_from_textarea_point(idx, &buffer, signal.left_press_pos);
            let click_count =
                self.register_text_click(key, signal.left_press_pos.unwrap_or_default());
            self.apply_text_click_selection(key, &buffer, cursor, click_count);
            self.reset_caret_blink(key);
        } else if dragging {
            let cursor = self.cursor_from_textarea_point(idx, &buffer, self.mouse);
            self.set_text_cursor(key, cursor, true);
            self.reset_caret_blink(key);
        }
    }

    /// Post-layout pass: resolve textarea click/drag selection for every text area in
    /// the frame, using final geometry.
    ///
    /// On the DOM backend, skips a `RICH_TEXT_HOST`: this geometry (`cum_x`,
    /// from Rust's own text shaping) has no reason to match the *browser's*
    /// text layout there — the hosted `<div contenteditable>` renders its own
    /// glyphs, and its click/drag-to-select is handled by the browser
    /// natively (see `paint_dom.rs`'s `pending_selection`/`sync_richtext_
    /// caret`). Applying this anyway wouldn't merely be redundant: it would
    /// compute a possibly-wrong cursor from mismatched pixel math and then,
    /// on the next repaint, forcibly override the browser's own — already
    /// correct — caret with it. Native rendering has no such mismatch (it's
    /// the one measuring the glyphs *and* the one drawing them), so a
    /// `RICH_TEXT_HOST` there keeps using this same geometry as any other
    /// textarea — this is not a `RICH_TEXT_HOST`-vs-not distinction on
    /// native, only on the DOM backend specifically.
    pub(super) fn apply_textarea_mouse_selections(&mut self) {
        for frame_pos in 0..self.frame_boxes.len() {
            let idx = self.frame_boxes[frame_pos];
            let flags = self.boxes[idx].flags;
            if !flags.contains(UIBoxFlags::MULTILINE) {
                continue;
            }
            #[cfg(feature = "dom")]
            if flags.contains(UIBoxFlags::RICH_TEXT_HOST) && self.dom.is_some() {
                continue;
            }
            self.apply_textarea_mouse_selection_for(idx);
        }
    }

    fn register_text_click(&mut self, key: UiKey, pos: Point) -> u8 {
        const DOUBLE_CLICK_MAX_SECONDS: f64 = 0.5;
        const DOUBLE_CLICK_MAX_DISTANCE: f32 = 5.0;

        let now = self.now_seconds();
        let streak = self.text_click_streak;
        let dx = pos.x - streak.pos.x;
        let dy = pos.y - streak.pos.y;
        let same_target = streak.key == key
            && now - streak.time <= DOUBLE_CLICK_MAX_SECONDS
            && dx * dx + dy * dy <= DOUBLE_CLICK_MAX_DISTANCE * DOUBLE_CLICK_MAX_DISTANCE;
        let count = if same_target {
            streak.count.saturating_add(1).min(3)
        } else {
            1
        };
        self.text_click_streak = TextClickStreak {
            key,
            pos,
            time: now,
            count,
        };
        count
    }

    fn apply_text_click_selection(&mut self, key: UiKey, buffer: &str, cursor: usize, count: u8) {
        match count {
            1 => self.set_text_cursor(key, cursor, false),
            2 => self.set_text_selection_range(key, text_word_range(buffer, cursor)),
            _ => self.set_text_selection_range(key, text_line_range(buffer, cursor)),
        }
    }

    fn set_text_selection_range(&mut self, key: UiKey, range: (usize, usize)) {
        self.break_undo_coalescing(key);
        let state = self.text_edit_states.entry(key).or_default();
        state.cursor = range.1;
        state.selection = if range.0 == range.1 {
            None
        } else {
            Some(TextSelection {
                anchor: range.0,
                cursor: range.1,
            })
        };
        state.desired_column = None;
    }

    pub(super) fn cursor_from_line_edit_point(
        &mut self,
        handle: UIBoxHandle,
        buffer: &str,
        point: Option<Point>,
    ) -> usize {
        let Some(point) = point else {
            return char_count(buffer);
        };
        let rect = self.boxes[handle.idx].rect;
        let padding = self.boxes[handle.idx].padding;
        let style = self.boxes[handle.idx].style;
        let scroll_x = self.boxes[handle.idx].scroll.x;
        let local_x = (point.x - rect.x0 - padding.left - style.margin + scroll_x).max(0.0);
        self.cursor_from_x(buffer, style.font_size, local_x)
    }

    pub(super) fn cursor_from_textarea_point(
        &mut self,
        idx: usize,
        buffer: &str,
        point: Option<Point>,
    ) -> usize {
        let Some(point) = point else {
            return char_count(buffer);
        };
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let style = self.boxes[idx].style;
        let visual_line = self.textarea_visual_line_from_point(idx, point);
        let key = self.ensure_layout_for_box(idx, buffer);
        let ranges = self.layout_ranges(key);
        let visual_line = visual_line.min(ranges.len().saturating_sub(1));
        let line_padding_left = self
            .textarea_line_rect(idx, visual_line)
            .map(|line| line.padding.left)
            .unwrap_or(0.0);
        let local_x = (point.x - rect.x0 - padding.left - style.margin - line_padding_left
            + self.boxes[idx].scroll.x)
            .max(0.0);
        self.layout_cursor_from_x(key, visual_line, local_x)
    }

    pub(super) fn textarea_wrap_width(&self, idx: usize) -> f32 {
        if self.boxes[idx].flags.contains(UIBoxFlags::NO_WRAP_X) {
            return 0.0;
        }
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let style = self.boxes[idx].style;
        (rect.x1 - rect.x0 - padding.horizontal() - style.margin * 2.0).max(0.0)
    }

    /// The visual line under `point`, clamped into range. A binary search over
    /// the cached row extents rather than a walk of the row boxes: it is O(log
    /// n) instead of O(n) per click, and it still answers once the clicked
    /// line is one the emit window skipped (an autoscrolling drag, say).
    pub(super) fn textarea_visual_line_from_point(&self, idx: usize, point: Point) -> usize {
        let local_y = point.y - self.boxes[idx].rect.y0 - self.boxes[idx].padding.top
            + self.boxes[idx].scroll.y;
        let Some(layout) = self.editor_layouts.get(&self.boxes[idx].key) else {
            let line_h = self.theme.size_text + 6.0;
            return (local_y / line_h).floor().max(0.0) as usize;
        };
        if layout.lines.is_empty() {
            return 0;
        }
        let gap = self.boxes[idx].child_gap;
        let line_idx = layout.line_at_offset(local_y, gap);
        // A point in the `child_gap` between two rows belongs to the row below
        // it, which is where walking the boxes in order used to land it.
        if local_y > layout.line_top(line_idx, gap) + layout.lines[line_idx].row_height()
            && line_idx + 1 < layout.lines.len()
        {
            return line_idx + 1;
        }
        line_idx
    }

    pub(super) fn cursor_from_x(&mut self, text: &str, font_size: f32, x: f32) -> usize {
        let mut last = 0;
        for idx in 0..=char_count(text) {
            let prefix = substring_chars(text, (0, idx));
            let width = self.text_size(font_size, &prefix).0;
            if width > x {
                return if idx == 0 { 0 } else { last };
            }
            last = idx;
        }
        last
    }

    /// Keep the focused text area scrolled so the caret stays visible. This lives
    /// outside the draw path (which is skipped when there is no drawer, e.g. in tests)
    /// so caret-follow scrolling is a real layout behavior, not a rendering side effect.
    /// Runs after layout so box rects and the cached line layout are current.
    pub(super) fn update_focused_textarea_scroll(&mut self) {
        let Some(key) = self.focus_key else {
            return;
        };
        let Some(idx) = self.box_from_key(key) else {
            return;
        };
        let flags = self.boxes[idx].flags;
        if !flags.accepts_text_input() {
            return;
        }
        if flags.contains(UIBoxFlags::LINE_EDIT) {
            self.update_focused_line_edit_scroll(idx, key);
            return;
        }

        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let margin = self.boxes[idx].style.margin;
        let text = self.boxes[idx].string.clone().unwrap_or_default();
        let cursor = self
            .text_edit_states
            .get(&key)
            .map(|state| state.cursor)
            .unwrap_or_else(|| char_count(&text))
            .min(char_count(&text));

        // Only follow the caret when it actually moves (typing, arrows, clicks). If the
        // caret is unchanged, leave scrolling alone so the user can freely scroll above
        // or below the cursor without it snapping back.
        if let Some(state) = self.text_edit_states.get_mut(&key) {
            if state.scroll_follow_cursor == Some(cursor) {
                return;
            }
            state.scroll_follow_cursor = Some(cursor);
        }

        self.ensure_layout_for_box(idx, &text);
        let ranges = self.layout_ranges(key);
        if ranges.is_empty() {
            return;
        }
        let (visual_line, _) = self.visual_line_col_from_cursor_with_ranges(&ranges, cursor);
        let visual_line = visual_line.min(ranges.len() - 1);

        let content_x0 = rect.x0 + padding.left + margin;
        let content_x1 = rect.x1 - padding.right - margin;
        let line_padding_left = self
            .textarea_line_rect(idx, visual_line)
            .map(|line| line.padding.left)
            .unwrap_or(0.0);
        let caret_x = line_padding_left + self.layout_caret_x(key, visual_line, cursor);
        self.keep_textarea_caret_visible(idx, caret_x, content_x0, content_x1);

        // Caret y in 0-based content space. `line_top` is the prefix sum this
        // used to re-add a line at a time, which made following the caret in a
        // long note O(lines) on every keystroke.
        let gap = self.boxes[idx].child_gap;
        let Some(layout) = self.editor_layouts.get(&key) else {
            return;
        };
        let caret_top = layout.line_top(visual_line, gap);
        let caret_bottom = caret_top + layout.lines[visual_line].row_height();
        self.keep_textarea_caret_visible_y(idx, caret_top, caret_bottom);
    }

    pub(super) fn keep_textarea_caret_visible_y(
        &mut self,
        idx: usize,
        caret_top: f32,
        caret_bottom: f32,
    ) {
        if !self.boxes[idx].flags.scrolls_y() {
            return;
        }
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let margin = self.boxes[idx].style.margin;
        let visible_height = (rect.height() - padding.vertical() - margin * 2.0).max(1.0);
        let scroll = self.boxes[idx].scroll.y;
        let mut target = self.boxes[idx].scroll_target.y;
        if caret_top < scroll {
            target = caret_top.max(0.0);
        } else if caret_bottom > scroll + visible_height {
            target = (caret_bottom - visible_height + 1.0).max(0.0);
        }
        target = target.clamp(0.0, self.boxes[idx].scroll_max.y);
        if (target - self.boxes[idx].scroll_target.y).abs() > f32::EPSILON {
            self.boxes[idx].scroll_target.y = target;
            self.request_repaint();
        }
    }

    pub(super) fn keep_textarea_caret_visible(
        &mut self,
        idx: usize,
        caret_content_x: f32,
        content_x0: f32,
        content_x1: f32,
    ) {
        if !self.boxes[idx].flags.scrolls_x() {
            return;
        }
        let visible_width = (content_x1 - content_x0).max(1.0);
        let scroll = self.boxes[idx].scroll.x;
        let mut target = self.boxes[idx].scroll_target.x;
        if caret_content_x < scroll {
            target = caret_content_x.max(0.0);
        } else if caret_content_x > scroll + visible_width {
            target = (caret_content_x - visible_width + 1.0).max(0.0);
        }
        target = target.clamp(0.0, self.boxes[idx].scroll_max.x);
        if (target - self.boxes[idx].scroll_target.x).abs() > f32::EPSILON {
            self.boxes[idx].scroll_target.x = target;
            self.request_repaint();
        }
    }

    /// Horizontal caret-follow scrolling for a single-line edit. Unlike a textarea it has
    /// no `SCROLL_X` flag (so no scrollbar and the layout never touches its `scroll.x`);
    /// we own `scroll_target.x` directly and let the shared scroll animation glide to it.
    /// The offset is consumed when drawing the text/caret/selection and when hit-testing.
    fn update_focused_line_edit_scroll(&mut self, idx: usize, key: UiKey) {
        let rect = self.boxes[idx].rect;
        let padding = self.boxes[idx].padding;
        let margin = self.boxes[idx].style.margin;
        let font_size = self.boxes[idx].style.font_size;
        let text = self.boxes[idx].display_string.clone().unwrap_or_default();
        let text_len = char_count(&text);
        let cursor = self
            .text_edit_states
            .get(&key)
            .map(|state| state.cursor)
            .unwrap_or(text_len)
            .min(text_len);

        // Follow the caret only when it actually moves, so the view doesn't snap back
        // while the cursor is parked (mirrors the textarea behavior).
        if let Some(state) = self.text_edit_states.get_mut(&key) {
            if state.scroll_follow_cursor == Some(cursor) {
                return;
            }
            state.scroll_follow_cursor = Some(cursor);
        }

        let caret_x = self
            .text_size(font_size, &substring_chars(&text, (0, cursor)))
            .0;
        let total_width = self.text_size(font_size, &text).0;
        let visible_width = (rect.width() - padding.horizontal() - margin * 2.0).max(1.0);
        let max_scroll = (total_width - visible_width).max(0.0);

        let mut target = self.boxes[idx].scroll_target.x;
        if caret_x < target {
            target = caret_x;
        } else if caret_x > target + visible_width {
            target = caret_x - visible_width + 1.0;
        }
        target = target.clamp(0.0, max_scroll);
        if (target - self.boxes[idx].scroll_target.x).abs() > f32::EPSILON {
            self.boxes[idx].scroll_target.x = target;
            self.request_repaint();
        }
    }
}

#[cfg(test)]
mod image_line_tests {
    use super::{SizeAxis, parse_image_line};

    #[test]
    fn parses_h_and_w_with_h_precedence() {
        // No size hint.
        let (key, size) = parse_image_line("![](./blob/a.png)").unwrap();
        assert_eq!(key, "./blob/a.png");
        assert!(size.is_none());

        // h= pins height.
        let (_, size) = parse_image_line("![alt](./blob/a.png?h=240)").unwrap();
        assert!(matches!(size, Some((SizeAxis::Height, h)) if h == 240.0));

        // w= pins width.
        let (_, size) = parse_image_line("![](./blob/a.png?w=320)").unwrap();
        assert!(matches!(size, Some((SizeAxis::Width, w)) if w == 320.0));

        // Both present → height wins (the default axis).
        let (_, size) = parse_image_line("![](./blob/a.png?w=320&h=240)").unwrap();
        assert!(matches!(size, Some((SizeAxis::Height, h)) if h == 240.0));

        // Query without a size param → None hint, key stripped of query.
        let (key, size) = parse_image_line("![](./blob/a.png?x=1)").unwrap();
        assert_eq!(key, "./blob/a.png");
        assert!(size.is_none());
    }

    #[test]
    fn rejects_non_image_lines() {
        assert!(parse_image_line("just text").is_none());
        assert!(parse_image_line("[link](./x)").is_none()); // not an image (`!` missing)
        assert!(parse_image_line("![](  )").is_none()); // whitespace url
        assert!(parse_image_line("![](./blob/a.png) trailing").is_none());
    }
}
