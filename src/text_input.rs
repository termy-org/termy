use crate::gpui::{
    Bounds, ElementInputHandler, Entity, EntityInputHandler, Font, Hsla, IntoElement, PaintQuad,
    Pixels, ShapedLine, Styled, TextAlign, TextRun, UTF16Selection, UnderlineStyle, canvas, fill,
    point, px, size,
};
use std::ops::Range;

const INLINE_INPUT_LINE_HEIGHT_MULTIPLIER: f32 = 1.35;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CharClass {
    Word,
    Whitespace,
    Other,
}

/// Shared text input state for single-line text fields.
/// Used by command palette, search, tab rename, and settings inputs.
#[derive(Clone, Debug)]
pub struct TextInputState {
    text: String,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    last_line_offset_x: Pixels,
}

#[allow(dead_code)]
impl TextInputState {
    #[inline]
    fn char_class(ch: char) -> CharClass {
        if ch.is_alphanumeric() || ch == '_' {
            CharClass::Word
        } else if ch.is_whitespace() {
            CharClass::Whitespace
        } else {
            CharClass::Other
        }
    }

    pub fn new(text: String) -> Self {
        let mut state = Self {
            text,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            last_line_offset_x: px(0.0),
        };
        state.move_to_end();
        state
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: String) {
        self.text = text;
        self.marked_range = None;
        self.selection_reversed = false;
        self.invalidate_layout();
        self.move_to_end();
    }

    pub fn clear(&mut self) {
        self.set_text(String::new());
    }

    pub fn move_to_end(&mut self) {
        self.set_cursor_utf8(self.text.len());
    }

    pub fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    pub fn selected_range(&self) -> Range<usize> {
        self.selected_range.clone()
    }

    pub fn select_all(&mut self) {
        self.selection_reversed = false;
        self.selected_range = 0..self.text.len();
    }

    pub fn marked_range(&self) -> Option<Range<usize>> {
        self.marked_range.clone()
    }

    fn set_cursor_utf8(&mut self, offset: usize) {
        let offset = Self::clamp_utf8_index(&self.text, offset);
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.marked_range = None;
    }

    pub fn select_to_utf8(&mut self, offset: usize) {
        let offset = Self::clamp_utf8_index(&self.text, offset);
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.marked_range = None;
    }

    pub fn set_cursor_utf16(&mut self, offset: usize) {
        let utf8_offset = Self::utf16_to_utf8_in_text(&self.text, offset);
        self.set_cursor_utf8(utf8_offset);
    }

    pub fn select_to_utf16(&mut self, offset: usize) {
        let utf8_offset = Self::utf16_to_utf8_in_text(&self.text, offset);
        self.select_to_utf8(utf8_offset);
    }

    fn previous_char_boundary(&self, offset: usize) -> usize {
        if offset == 0 {
            return 0;
        }

        let mut index = offset.min(self.text.len());
        while index > 0 {
            index -= 1;
            if self.text.is_char_boundary(index) {
                return index;
            }
        }
        0
    }

    fn next_char_boundary(&self, offset: usize) -> usize {
        if offset >= self.text.len() {
            return self.text.len();
        }

        let mut index = offset + 1;
        while index < self.text.len() {
            if self.text.is_char_boundary(index) {
                return index;
            }
            index += 1;
        }
        self.text.len()
    }

    fn previous_word_boundary(&self, offset: usize) -> usize {
        if offset == 0 {
            return 0;
        }

        let mut boundary = 0;
        let mut seen_word = false;
        for (idx, ch) in self.text[..offset].char_indices().rev() {
            if Self::char_class(ch) == CharClass::Word {
                seen_word = true;
                boundary = idx;
                continue;
            }
            if seen_word {
                boundary = idx + ch.len_utf8();
                break;
            }
            boundary = idx;
        }
        boundary
    }

    fn next_word_boundary(&self, offset: usize) -> usize {
        if offset >= self.text.len() {
            return self.text.len();
        }

        let mut seen_word = false;
        for (rel_idx, ch) in self.text[offset..].char_indices() {
            let is_word = Self::char_class(ch) == CharClass::Word;
            if is_word {
                seen_word = true;
            } else if seen_word {
                return offset + rel_idx;
            }
        }
        self.text.len()
    }

    fn select_range_utf8(&mut self, range: Range<usize>) {
        let start = Self::clamp_utf8_index(&self.text, range.start.min(self.text.len()));
        let end = Self::clamp_utf8_index(&self.text, range.end.min(self.text.len()));
        if end < start {
            self.selected_range = end..start;
        } else {
            self.selected_range = start..end;
        }
        self.selection_reversed = false;
        self.marked_range = None;
    }

    fn token_range_at_utf8(&self, offset: usize) -> Range<usize> {
        if self.text.is_empty() {
            return 0..0;
        }

        let mut anchor = Self::clamp_utf8_index(&self.text, offset.min(self.text.len()));
        if anchor == self.text.len() && anchor > 0 {
            anchor = self.previous_char_boundary(anchor);
        }
        if anchor >= self.text.len() {
            return self.text.len()..self.text.len();
        }

        let Some(anchor_char) = self.text[anchor..].chars().next() else {
            return self.text.len()..self.text.len();
        };
        let class = Self::char_class(anchor_char);

        let mut start = anchor;
        while start > 0 {
            let prev = self.previous_char_boundary(start);
            let Some(prev_char) = self.text[prev..start].chars().next() else {
                break;
            };
            if Self::char_class(prev_char) != class {
                break;
            }
            start = prev;
        }

        let mut end = self.next_char_boundary(anchor);
        while end < self.text.len() {
            let next_end = self.next_char_boundary(end);
            let Some(next_char) = self.text[end..next_end].chars().next() else {
                break;
            };
            if Self::char_class(next_char) != class {
                break;
            }
            end = next_end;
        }

        start..end
    }

    pub fn select_token_at_utf16(&mut self, offset_utf16: usize) {
        let utf8_offset = Self::utf16_to_utf8_in_text(&self.text, offset_utf16);
        let range = self.token_range_at_utf8(utf8_offset);
        self.select_range_utf8(range);
    }

    fn delete_range_utf8(&mut self, range: Range<usize>) {
        if range.start >= range.end || range.end > self.text.len() {
            return;
        }
        self.text.replace_range(range.clone(), "");
        self.set_cursor_utf8(range.start);
        self.invalidate_layout();
    }

    fn delete_selected_or(&mut self, fallback: Range<usize>) {
        let range = if self.selected_range.is_empty() {
            fallback
        } else {
            self.selected_range.clone()
        };
        self.delete_range_utf8(range);
    }

    pub fn move_left(&mut self) {
        if !self.selected_range.is_empty() {
            self.set_cursor_utf8(self.selected_range.start);
            return;
        }
        let cursor = self.cursor_offset();
        self.set_cursor_utf8(self.previous_char_boundary(cursor));
    }

    pub fn move_right(&mut self) {
        if !self.selected_range.is_empty() {
            self.set_cursor_utf8(self.selected_range.end);
            return;
        }
        let cursor = self.cursor_offset();
        self.set_cursor_utf8(self.next_char_boundary(cursor));
    }

    pub fn select_left(&mut self) {
        let cursor = self.cursor_offset();
        self.select_to_utf8(self.previous_char_boundary(cursor));
    }

    pub fn select_right(&mut self) {
        let cursor = self.cursor_offset();
        self.select_to_utf8(self.next_char_boundary(cursor));
    }

    pub fn move_to_start(&mut self) {
        self.set_cursor_utf8(0);
    }

    pub fn delete_backward(&mut self) {
        let cursor = self.cursor_offset();
        let start = self.previous_char_boundary(cursor);
        self.delete_selected_or(start..cursor);
    }

    pub fn delete_forward(&mut self) {
        let cursor = self.cursor_offset();
        let end = self.next_char_boundary(cursor);
        self.delete_selected_or(cursor..end);
    }

    pub fn delete_word_backward(&mut self) {
        let cursor = self.cursor_offset();
        let start = self.previous_word_boundary(cursor);
        self.delete_selected_or(start..cursor);
    }

    pub fn delete_word_forward(&mut self) {
        let cursor = self.cursor_offset();
        let end = self.next_word_boundary(cursor);
        self.delete_selected_or(cursor..end);
    }

    pub fn delete_to_start(&mut self) {
        let cursor = self.cursor_offset();
        self.delete_selected_or(0..cursor);
    }

    pub fn delete_to_end(&mut self) {
        let cursor = self.cursor_offset();
        self.delete_selected_or(cursor..self.text.len());
    }

    fn invalidate_layout(&mut self) {
        self.last_layout = None;
    }

    pub fn update_layout_cache(
        &mut self,
        bounds: Bounds<Pixels>,
        layout: Option<ShapedLine>,
        line_offset_x: Pixels,
    ) {
        self.last_bounds = Some(bounds);
        self.last_layout = layout;
        self.last_line_offset_x = line_offset_x;
    }

    fn clamp_utf8_index(text: &str, index: usize) -> usize {
        let mut index = index.min(text.len());
        while index > 0 && !text.is_char_boundary(index) {
            index -= 1;
        }
        index
    }

    fn utf16_to_utf8_in_text(text: &str, utf16_offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in text.chars() {
            if utf16_count >= utf16_offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        Self::clamp_utf8_index(text, utf8_offset)
    }

    fn utf8_to_utf16_in_text(text: &str, utf8_offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        let clamped_utf8 = Self::clamp_utf8_index(text, utf8_offset);

        for ch in text.chars() {
            if utf8_count >= clamped_utf8 {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_from_utf16_for_text(text: &str, range_utf16: &Range<usize>) -> Range<usize> {
        let start = Self::utf16_to_utf8_in_text(text, range_utf16.start);
        let end = Self::utf16_to_utf8_in_text(text, range_utf16.end);
        if end < start { end..start } else { start..end }
    }

    fn range_to_utf16_for_text(text: &str, range_utf8: &Range<usize>) -> Range<usize> {
        let start = Self::utf8_to_utf16_in_text(text, range_utf8.start);
        let end = Self::utf8_to_utf16_in_text(text, range_utf8.end);
        if end < start { end..start } else { start..end }
    }

    pub fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        Self::range_from_utf16_for_text(&self.text, range_utf16)
    }

    pub fn range_to_utf16(&self, range_utf8: &Range<usize>) -> Range<usize> {
        Self::range_to_utf16_for_text(&self.text, range_utf8)
    }

    pub fn utf8_to_utf16(&self, utf8_offset: usize) -> usize {
        Self::utf8_to_utf16_in_text(&self.text, utf8_offset)
    }

    fn replacement_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range())
    }

    pub fn text_for_range(
        &self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
    ) -> String {
        let range = self.range_from_utf16(&range_utf16);
        adjusted_range.replace(self.range_to_utf16(&range));
        self.text[range].to_string()
    }

    pub fn bounds_for_range(
        &self,
        range_utf16: Range<usize>,
        fallback_bounds: Bounds<Pixels>,
    ) -> Bounds<Pixels> {
        let bounds = self.last_bounds.unwrap_or(fallback_bounds);
        let range = self.range_from_utf16(&range_utf16);
        let (start_x, end_x) = if let Some(layout) = self.last_layout.as_ref() {
            (
                layout.x_for_index(range.start),
                layout.x_for_index(range.end),
            )
        } else {
            (px(0.0), px(0.0))
        };

        Bounds::from_corners(
            point(
                bounds.left() + self.last_line_offset_x + start_x,
                bounds.top(),
            ),
            point(
                bounds.left() + self.last_line_offset_x + end_x,
                bounds.bottom(),
            ),
        )
    }

    pub fn selected_text_range(&self) -> UTF16Selection {
        UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        }
    }

    pub fn marked_text_range_utf16(&self) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    pub fn character_index_for_point(&self, point: crate::gpui::Point<Pixels>) -> usize {
        if self.text.is_empty() {
            return 0;
        }

        let Some(bounds) = self.last_bounds else {
            return self.range_to_utf16(&self.selected_range).start;
        };

        if point.y < bounds.top() {
            return 0;
        }
        if point.y > bounds.bottom() {
            return self.utf8_to_utf16(self.text.len());
        }

        let text_left = bounds.left() + self.last_line_offset_x;
        let local_x = if point.x <= text_left {
            px(0.0)
        } else {
            point.x - text_left
        };

        let utf8_index = self
            .last_layout
            .as_ref()
            .map(|layout| layout.closest_index_for_x(local_x))
            .unwrap_or(0);
        self.utf8_to_utf16(utf8_index)
    }

    pub fn unmark_text(&mut self) {
        self.marked_range = None;
    }

    pub fn replace_text_in_range(&mut self, range_utf16: Option<Range<usize>>, text: &str) {
        let range = self.replacement_range(range_utf16);
        self.text.replace_range(range.clone(), text);
        let cursor = range.start + text.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.invalidate_layout();
    }

    pub fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
    ) {
        let range = self.replacement_range(range_utf16);
        self.text.replace_range(range.clone(), new_text);

        if new_text.is_empty() {
            self.marked_range = None;
        } else {
            self.marked_range = Some(range.start..range.start + new_text.len());
        }

        self.selection_reversed = false;
        if let Some(local_selected_utf16) = new_selected_range_utf16 {
            let local_selected = Self::range_from_utf16_for_text(new_text, &local_selected_utf16);
            let selected_start = range.start + local_selected.start;
            let selected_end = range.start + local_selected.end;
            self.selected_range = selected_start..selected_end;
        } else {
            let cursor = range.start + new_text.len();
            self.selected_range = cursor..cursor;
        }
        self.invalidate_layout();
    }
}

/// Trait for views that can provide text input state for the TextInputElement.
pub trait TextInputProvider: 'static + Sized {
    /// Returns the current text input state, if any.
    fn text_input_state(&self) -> Option<&TextInputState>;

    /// Returns a mutable reference to the current text input state, if any.
    fn text_input_state_mut(&mut self) -> Option<&mut TextInputState>;

    /// Returns whether the cursor should be visible. Defaults to true.
    fn cursor_visible(&self) -> bool {
        true
    }

    /// Returns the cursor style (line width). Defaults to 1.0px (line cursor).
    fn cursor_width(&self, _font_size: f32) -> f32 {
        1.0
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextInputAlignment {
    Left,
    Center,
}

pub struct TextInputPrepaintState {
    line: Option<ShapedLine>,
    line_bounds: Bounds<Pixels>,
    line_offset_x: Pixels,
    selection: Option<PaintQuad>,
    cursor: Option<PaintQuad>,
}

pub struct TextInputElement<V: TextInputProvider> {
    view: Entity<V>,
    focus_handle: crate::gpui::FocusHandle,
    font: Font,
    font_size: Pixels,
    text_color: Hsla,
    selection_color: Hsla,
    alignment: TextInputAlignment,
}

impl<V: TextInputProvider> TextInputElement<V> {
    pub fn new(
        view: Entity<V>,
        focus_handle: crate::gpui::FocusHandle,
        font: Font,
        font_size: Pixels,
        text_color: Hsla,
        selection_color: Hsla,
        alignment: TextInputAlignment,
    ) -> Self {
        Self {
            view,
            focus_handle,
            font,
            font_size,
            text_color,
            selection_color,
            alignment,
        }
    }
}

impl<V: TextInputProvider + crate::gpui::Render + EntityInputHandler> IntoElement for TextInputElement<V> {
    type Element = crate::gpui::Canvas<TextInputPrepaintState>;

    fn into_element(self) -> Self::Element {
        let focus_handle = self.focus_handle;
        let prepaint_focus_handle = focus_handle.clone();
        let view = self.view;
        let prepaint_view = view.clone();
        let font = self.font;
        let font_size = self.font_size;
        let text_color = self.text_color;
        let selection_color = self.selection_color;
        let alignment = self.alignment;

        canvas(
            move |bounds, window, cx| {
                let font_size_value: f32 = font_size.into();
                let bounds_height: f32 = bounds.size.height.into();
                let target_line_height = (font_size_value * INLINE_INPUT_LINE_HEIGHT_MULTIPLIER)
                    .round()
                    .clamp(1.0, bounds_height.max(1.0));
                let line_height = px(target_line_height);
                let extra_height: f32 = (bounds.size.height - line_height).into();
                let vertical_offset = px(extra_height.max(0.0) * 0.5);
                let line_bounds = Bounds::new(
                    point(bounds.left(), bounds.top() + vertical_offset),
                    size(bounds.size.width, line_height),
                );

                let (
                    text,
                    selected_range,
                    cursor_offset,
                    marked_range,
                    focused,
                    cursor_visible,
                    cursor_width,
                ) = {
                    let view = prepaint_view.read(cx);
                    let focused = prepaint_focus_handle.is_focused(window);
                    let cursor_visible = view.cursor_visible();
                    let cursor_width = view.cursor_width(font_size_value);
                    view.text_input_state()
                        .map(|state| {
                            (
                                state.text().to_string(),
                                state.selected_range(),
                                state.cursor_offset(),
                                state.marked_range.clone(),
                                focused,
                                cursor_visible,
                                cursor_width,
                            )
                        })
                        .unwrap_or_else(|| {
                            (String::new(), 0..0, 0, None, focused, false, cursor_width)
                        })
                };

                let line = if text.is_empty() {
                    None
                } else {
                    let base_run = TextRun {
                        len: text.len(),
                        font: font.clone(),
                        color: text_color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };

                    let runs = if let Some(marked_range) = marked_range {
                        let marked_start = marked_range.start.min(text.len());
                        let marked_end = marked_range.end.min(text.len()).max(marked_start);
                        let mut runs = Vec::with_capacity(3);
                        if marked_start > 0 {
                            runs.push(TextRun {
                                len: marked_start,
                                ..base_run.clone()
                            });
                        }
                        if marked_end > marked_start {
                            runs.push(TextRun {
                                len: marked_end - marked_start,
                                underline: Some(UnderlineStyle {
                                    color: Some(text_color),
                                    thickness: px(1.0),
                                    wavy: false,
                                }),
                                ..base_run.clone()
                            });
                        }
                        if marked_end < text.len() {
                            runs.push(TextRun {
                                len: text.len() - marked_end,
                                ..base_run.clone()
                            });
                        }
                        runs
                    } else {
                        vec![base_run]
                    };

                    Some(window.text_system().shape_line(
                        text.clone().into(),
                        font_size,
                        &runs,
                        None,
                    ))
                };

                let line_width = line
                    .as_ref()
                    .map(|line| line.x_for_index(text.len()))
                    .unwrap_or(px(0.0));
                let line_offset_x = match alignment {
                    TextInputAlignment::Left => px(0.0),
                    TextInputAlignment::Center => {
                        let available_width: f32 = line_bounds.size.width.into();
                        let text_width: f32 = line_width.into();
                        px(((available_width - text_width).max(0.0) * 0.5).round())
                    }
                };

                let cursor_utf8 = cursor_offset.min(text.len());
                let selection_start = selected_range.start.min(text.len());
                let selection_end = selected_range.end.min(text.len());

                let selection = if selection_start < selection_end {
                    let start_x = line
                        .as_ref()
                        .map(|line| line.x_for_index(selection_start))
                        .unwrap_or(px(0.0));
                    let end_x = line
                        .as_ref()
                        .map(|line| line.x_for_index(selection_end))
                        .unwrap_or(px(0.0));
                    Some(fill(
                        Bounds::from_corners(
                            point(
                                line_bounds.left() + line_offset_x + start_x,
                                line_bounds.top(),
                            ),
                            point(
                                line_bounds.left() + line_offset_x + end_x,
                                line_bounds.bottom(),
                            ),
                        ),
                        selection_color,
                    ))
                } else {
                    None
                };

                let cursor = if focused && cursor_visible && selection_start == selection_end {
                    let cursor_x = line
                        .as_ref()
                        .map(|line| line.x_for_index(cursor_utf8))
                        .unwrap_or(px(0.0));
                    let cursor_x = px({
                        let x: f32 = cursor_x.into();
                        x.round()
                    });

                    Some(fill(
                        Bounds::new(
                            point(
                                line_bounds.left() + line_offset_x + cursor_x,
                                line_bounds.top(),
                            ),
                            size(px(cursor_width), line_bounds.size.height),
                        ),
                        text_color,
                    ))
                } else {
                    None
                };

                TextInputPrepaintState {
                    line,
                    line_bounds,
                    selection,
                    cursor,
                    line_offset_x,
                }
            },
            move |bounds, mut prepaint, window, cx| {
                window.handle_input(
                    &focus_handle,
                    ElementInputHandler::new(bounds, view.clone()),
                    cx,
                );

                if let Some(selection) = prepaint.selection.take() {
                    window.paint_quad(selection);
                }

                let line = if let Some(line) = prepaint.line.take() {
                    line.paint(
                        point(
                            prepaint.line_bounds.left() + prepaint.line_offset_x,
                            prepaint.line_bounds.top(),
                        ),
                        prepaint.line_bounds.size.height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .expect("failed to paint text input text");
                    Some(line)
                } else {
                    None
                };

                if let Some(cursor) = prepaint.cursor.take() {
                    window.paint_quad(cursor);
                }

                view.update(cx, |this, _cx| {
                    if let Some(state) = this.text_input_state_mut() {
                        state.update_layout_cache(
                            prepaint.line_bounds,
                            line,
                            prepaint.line_offset_x,
                        );
                    }
                });
            },
        )
        .size_full()
    }
}

/// Helper macro to implement EntityInputHandler for a type using TextInputProvider.
/// This reduces boilerplate when implementing text input for different views.
#[macro_export]
macro_rules! impl_text_input_handler {
    ($ty:ty) => {
        impl crate::gpui::EntityInputHandler for $ty {
            fn text_for_range(
                &mut self,
                range: std::ops::Range<usize>,
                adjusted_range: &mut Option<std::ops::Range<usize>>,
                _window: &mut crate::gpui::Window,
                _cx: &mut crate::gpui::Context<Self>,
            ) -> Option<String> {
                let state = $crate::text_input::TextInputProvider::text_input_state(self)?;
                Some(state.text_for_range(range, adjusted_range))
            }

            fn selected_text_range(
                &mut self,
                _ignore_disabled_input: bool,
                _window: &mut crate::gpui::Window,
                _cx: &mut crate::gpui::Context<Self>,
            ) -> Option<crate::gpui::UTF16Selection> {
                let state = $crate::text_input::TextInputProvider::text_input_state(self)?;
                Some(state.selected_text_range())
            }

            fn marked_text_range(
                &self,
                _window: &mut crate::gpui::Window,
                _cx: &mut crate::gpui::Context<Self>,
            ) -> Option<std::ops::Range<usize>> {
                let state = $crate::text_input::TextInputProvider::text_input_state(self)?;
                state.marked_text_range_utf16()
            }

            fn unmark_text(&mut self, _window: &mut crate::gpui::Window, _cx: &mut crate::gpui::Context<Self>) {
                if let Some(state) =
                    $crate::text_input::TextInputProvider::text_input_state_mut(self)
                {
                    state.unmark_text();
                }
            }

            fn replace_text_in_range(
                &mut self,
                range: Option<std::ops::Range<usize>>,
                text: &str,
                _window: &mut crate::gpui::Window,
                cx: &mut crate::gpui::Context<Self>,
            ) {
                if let Some(state) =
                    $crate::text_input::TextInputProvider::text_input_state_mut(self)
                {
                    state.replace_text_in_range(range, text);
                    cx.notify();
                }
            }

            fn replace_and_mark_text_in_range(
                &mut self,
                range: Option<std::ops::Range<usize>>,
                new_text: &str,
                new_selected_range: Option<std::ops::Range<usize>>,
                _window: &mut crate::gpui::Window,
                cx: &mut crate::gpui::Context<Self>,
            ) {
                if let Some(state) =
                    $crate::text_input::TextInputProvider::text_input_state_mut(self)
                {
                    state.replace_and_mark_text_in_range(range, new_text, new_selected_range);
                    cx.notify();
                }
            }

            fn bounds_for_range(
                &mut self,
                range_utf16: std::ops::Range<usize>,
                element_bounds: crate::gpui::Bounds<crate::gpui::Pixels>,
                _window: &mut crate::gpui::Window,
                _cx: &mut crate::gpui::Context<Self>,
            ) -> Option<crate::gpui::Bounds<crate::gpui::Pixels>> {
                let state = $crate::text_input::TextInputProvider::text_input_state(self)?;
                Some(state.bounds_for_range(range_utf16, element_bounds))
            }

            fn character_index_for_point(
                &mut self,
                point: crate::gpui::Point<crate::gpui::Pixels>,
                _window: &mut crate::gpui::Window,
                _cx: &mut crate::gpui::Context<Self>,
            ) -> Option<usize> {
                let state = $crate::text_input::TextInputProvider::text_input_state(self)?;
                Some(state.character_index_for_point(point))
            }

            fn accepts_text_input(
                &self,
                _window: &mut crate::gpui::Window,
                _cx: &mut crate::gpui::Context<Self>,
            ) -> bool {
                $crate::text_input::TextInputProvider::text_input_state(self).is_some()
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_range_conversion_handles_multibyte_text() {
        let state = TextInputState::new("a😄é".to_string());
        let utf16 = state.range_to_utf16(&(1..7));
        assert_eq!(utf16, 1..4);
        let utf8 = state.range_from_utf16(&utf16);
        assert_eq!(utf8, 1..7);
    }

    #[test]
    fn replace_text_uses_selection_when_no_range() {
        let mut state = TextInputState::new("hello".to_string());
        state.selected_range = 1..4;
        state.replace_text_in_range(None, "i");
        assert_eq!(state.text(), "hio");
        assert_eq!(state.selected_range(), 2..2);
    }

    #[test]
    fn replace_and_mark_sets_marked_and_selection() {
        let mut state = TextInputState::new("abcd".to_string());
        state.selected_range = 1..3;
        state.replace_and_mark_text_in_range(Some(1..3), "xy", Some(0..1));
        assert_eq!(state.text(), "axyd");
        assert_eq!(state.marked_range, Some(1..3));
        assert_eq!(state.selected_range(), 1..2);
    }

    #[test]
    fn unmark_clears_marked_range() {
        let mut state = TextInputState::new("abc".to_string());
        state.marked_range = Some(0..2);
        state.unmark_text();
        assert_eq!(state.marked_range, None);
    }

    #[test]
    fn delete_to_start_removes_prefix() {
        let mut state = TextInputState::new("hello world".to_string());
        state.set_cursor_utf8(5);
        state.delete_to_start();
        assert_eq!(state.text(), " world");
        assert_eq!(state.selected_range(), 0..0);
    }

    #[test]
    fn delete_word_backward_removes_previous_word() {
        let mut state = TextInputState::new("hello world".to_string());
        state.set_cursor_utf8(11);
        state.delete_word_backward();
        assert_eq!(state.text(), "hello ");
        assert_eq!(state.selected_range(), 6..6);
    }

    #[test]
    fn select_to_utf16_extends_selection() {
        let mut state = TextInputState::new("a😄b".to_string());
        state.set_cursor_utf16(1);
        state.select_to_utf16(4);
        assert_eq!(state.selected_range(), 1..6);
    }

    #[test]
    fn select_token_at_utf16_selects_word_and_whitespace_runs() {
        let mut state = TextInputState::new("hello  world".to_string());

        state.select_token_at_utf16(1);
        assert_eq!(state.selected_range(), 0..5);

        state.select_token_at_utf16(5);
        assert_eq!(state.selected_range(), 5..7);
    }

    #[test]
    fn select_token_at_utf16_handles_punctuation_and_end_of_line() {
        let mut state = TextInputState::new("foo==bar".to_string());

        state.select_token_at_utf16(3);
        assert_eq!(state.selected_range(), 3..5);

        state.select_token_at_utf16(8);
        assert_eq!(state.selected_range(), 5..8);
    }
}
