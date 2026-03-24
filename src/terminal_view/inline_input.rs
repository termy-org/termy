use super::*;
use gpui::{
    Bounds, ContentMask, ElementInputHandler, Entity, EntityInputHandler, Font, Hsla, IntoElement,
    PaintQuad, Pixels, ShapedLine, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window,
    canvas, fill, point, px, size,
};
use std::ops::Range;

const INLINE_INPUT_LINE_HEIGHT_MULTIPLIER: f32 = 1.35;

fn ime_marked_text_range_utf16(marked_text: Option<&str>) -> Option<Range<usize>> {
    marked_text.map(|marked| 0..marked.encode_utf16().count())
}

fn terminal_ime_selected_text_range(ime_selected_range: Option<Range<usize>>) -> UTF16Selection {
    UTF16Selection {
        range: ime_selected_range.unwrap_or(0..0),
        reversed: false,
    }
}

fn ime_candidate_bounds(
    cursor: Bounds<Pixels>,
    element_bounds: Bounds<Pixels>,
    range_start_utf16: usize,
    cell_width: f32,
) -> Bounds<Pixels> {
    let mut bounds = Bounds::new(
        point(
            element_bounds.origin.x + cursor.origin.x,
            element_bounds.origin.y + cursor.origin.y,
        ),
        cursor.size,
    );
    bounds.origin.x += px(range_start_utf16 as f32 * cell_width);
    bounds
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InlineInputCharClass {
    Word,
    Whitespace,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InlineInputTarget {
    CommandPalette,
    RenameTab,
    Search,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InlineInputNotifyTarget {
    Parent,
    Overlay,
}

#[derive(Clone, Debug)]
pub(super) struct InlineInputState {
    text: String,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    last_line_offset_x: Pixels,
}

impl InlineInputState {
    #[inline]
    fn char_class(ch: char) -> InlineInputCharClass {
        if ch.is_alphanumeric() || ch == '_' {
            InlineInputCharClass::Word
        } else if ch.is_whitespace() {
            InlineInputCharClass::Whitespace
        } else {
            InlineInputCharClass::Other
        }
    }

    pub(super) fn new(text: String) -> Self {
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

    pub(super) fn text(&self) -> &str {
        &self.text
    }

    pub(super) fn set_text(&mut self, text: String) {
        self.text = text;
        self.marked_range = None;
        self.selection_reversed = false;
        self.invalidate_layout();
        self.move_to_end();
    }

    pub(super) fn clear(&mut self) {
        self.set_text(String::new());
    }

    pub(super) fn move_to_end(&mut self) {
        self.set_cursor_utf8(self.text.len());
    }

    pub(super) fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    pub(super) fn selected_range(&self) -> Range<usize> {
        self.selected_range.clone()
    }

    pub(super) fn selected_text(&self) -> Option<String> {
        (!self.selected_range.is_empty())
            .then(|| self.text[self.selected_range.clone()].to_string())
    }

    pub(super) fn select_all(&mut self) {
        self.selection_reversed = false;
        self.selected_range = 0..self.text.len();
    }

    fn set_cursor_utf8(&mut self, offset: usize) {
        let offset = Self::clamp_utf8_index(&self.text, offset);
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.marked_range = None;
    }

    fn select_to_utf8(&mut self, offset: usize) {
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

    fn set_cursor_utf16(&mut self, offset: usize) {
        let utf8_offset = Self::utf16_to_utf8_in_text(&self.text, offset);
        self.set_cursor_utf8(utf8_offset);
    }

    fn select_to_utf16(&mut self, offset: usize) {
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
            if Self::char_class(ch) == InlineInputCharClass::Word {
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
            let is_word = Self::char_class(ch) == InlineInputCharClass::Word;
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

    fn select_token_at_utf16(&mut self, offset_utf16: usize) {
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

    pub(super) fn move_left(&mut self) {
        if !self.selected_range.is_empty() {
            self.set_cursor_utf8(self.selected_range.start);
            return;
        }
        let cursor = self.cursor_offset();
        self.set_cursor_utf8(self.previous_char_boundary(cursor));
    }

    pub(super) fn move_right(&mut self) {
        if !self.selected_range.is_empty() {
            self.set_cursor_utf8(self.selected_range.end);
            return;
        }
        let cursor = self.cursor_offset();
        self.set_cursor_utf8(self.next_char_boundary(cursor));
    }

    pub(super) fn select_left(&mut self) {
        let cursor = self.cursor_offset();
        self.select_to_utf8(self.previous_char_boundary(cursor));
    }

    pub(super) fn select_right(&mut self) {
        let cursor = self.cursor_offset();
        self.select_to_utf8(self.next_char_boundary(cursor));
    }

    pub(super) fn move_to_start(&mut self) {
        self.set_cursor_utf8(0);
    }

    pub(super) fn delete_backward(&mut self) {
        let cursor = self.cursor_offset();
        let start = self.previous_char_boundary(cursor);
        self.delete_selected_or(start..cursor);
    }

    pub(super) fn delete_forward(&mut self) {
        let cursor = self.cursor_offset();
        let end = self.next_char_boundary(cursor);
        self.delete_selected_or(cursor..end);
    }

    pub(super) fn delete_word_backward(&mut self) {
        let cursor = self.cursor_offset();
        let start = self.previous_word_boundary(cursor);
        self.delete_selected_or(start..cursor);
    }

    pub(super) fn delete_word_forward(&mut self) {
        let cursor = self.cursor_offset();
        let end = self.next_word_boundary(cursor);
        self.delete_selected_or(cursor..end);
    }

    pub(super) fn delete_to_start(&mut self) {
        let cursor = self.cursor_offset();
        self.delete_selected_or(0..cursor);
    }

    pub(super) fn delete_to_end(&mut self) {
        let cursor = self.cursor_offset();
        self.delete_selected_or(cursor..self.text.len());
    }

    fn invalidate_layout(&mut self) {
        self.last_layout = None;
    }

    fn update_layout_cache(
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

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        Self::range_from_utf16_for_text(&self.text, range_utf16)
    }

    fn range_to_utf16(&self, range_utf8: &Range<usize>) -> Range<usize> {
        Self::range_to_utf16_for_text(&self.text, range_utf8)
    }

    fn utf8_to_utf16(&self, utf8_offset: usize) -> usize {
        Self::utf8_to_utf16_in_text(&self.text, utf8_offset)
    }

    fn replacement_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range())
    }

    pub(super) fn text_for_range(
        &self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
    ) -> String {
        let range = self.range_from_utf16(&range_utf16);
        adjusted_range.replace(self.range_to_utf16(&range));
        self.text[range].to_string()
    }

    pub(super) fn bounds_for_range(
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

    pub(super) fn selected_text_range(&self) -> UTF16Selection {
        UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        }
    }

    pub(super) fn marked_text_range(&self) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    pub(super) fn character_index_for_point(&self, point: gpui::Point<Pixels>) -> usize {
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

    pub(super) fn unmark_text(&mut self) {
        self.marked_range = None;
    }

    pub(super) fn replace_text_in_range(&mut self, range_utf16: Option<Range<usize>>, text: &str) {
        let range = self.replacement_range(range_utf16);
        self.text.replace_range(range.clone(), text);
        let cursor = range.start + text.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.invalidate_layout();
    }

    pub(super) fn replace_and_mark_text_in_range(
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

pub(super) struct InlineInputElement {
    view: Entity<TerminalView>,
    focus_handle: FocusHandle,
    font: Font,
    font_size: Pixels,
    text_color: Hsla,
    selection_color: Hsla,
    alignment: InlineInputAlignment,
}

// Retained for upcoming inline input layout variants and to keep call sites stable
// while alignment options are wired through additional UI surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) enum InlineInputAlignment {
    Left,
    Center,
}

impl InlineInputElement {
    pub(super) fn new(
        view: Entity<TerminalView>,
        focus_handle: FocusHandle,
        font: Font,
        font_size: Pixels,
        text_color: Hsla,
        selection_color: Hsla,
        alignment: InlineInputAlignment,
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

pub(super) struct InlineInputPrepaintState {
    line: Option<ShapedLine>,
    line_bounds: Bounds<Pixels>,
    line_offset_x: Pixels,
    selection: Option<PaintQuad>,
    cursor: Option<PaintQuad>,
}

impl IntoElement for InlineInputElement {
    type Element = gpui::Canvas<InlineInputPrepaintState>;

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
                    cursor_style,
                ) = {
                    let view = prepaint_view.read(cx);
                    let focused = prepaint_focus_handle.is_focused(window);
                    let cursor_visible = view.cursor_visible_for_focus(focused);
                    let cursor_style = view.cursor_style;
                    view.active_inline_input_state()
                        .map(|state| {
                            (
                                state.text().to_string(),
                                state.selected_range(),
                                state.cursor_offset(),
                                state.marked_range.clone(),
                                focused,
                                cursor_visible,
                                cursor_style,
                            )
                        })
                        .unwrap_or_else(|| {
                            (String::new(), 0..0, 0, None, focused, false, cursor_style)
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
                let cursor_utf8_early = cursor_offset.min(text.len());
                let line_offset_x = match alignment {
                    InlineInputAlignment::Left => {
                        let available_width: f32 = line_bounds.size.width.into();
                        let prev_offset: f32 = prepaint_view
                            .read(cx)
                            .active_inline_input_state()
                            .map(|s| -> f32 { s.last_line_offset_x.into() })
                            .unwrap_or(0.0);
                        let cursor_x: f32 = line
                            .as_ref()
                            .map(|l| -> f32 { l.x_for_index(cursor_utf8_early).into() })
                            .unwrap_or(0.0);
                        let visible_cursor_x = cursor_x + prev_offset;
                        let padding = 4.0_f32;
                        let new_offset = if visible_cursor_x < 0.0 {
                            -(cursor_x - padding).max(0.0)
                        } else if visible_cursor_x > available_width - padding {
                            -(cursor_x - available_width + padding)
                        } else {
                            prev_offset
                        };
                        px(new_offset.round())
                    }
                    InlineInputAlignment::Center => {
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
                    let cursor_width = match cursor_style {
                        AppCursorStyle::Line => px(1.0),
                        AppCursorStyle::Block => {
                            let fallback_width = (font_size_value * 0.62).round().max(1.0);
                            let width = text
                                .get(cursor_utf8..)
                                .and_then(|slice| slice.chars().next())
                                .map(|ch| cursor_utf8 + ch.len_utf8())
                                .and_then(|next_utf8| {
                                    line.as_ref()
                                        .map(|line| line.x_for_index(next_utf8) - cursor_x)
                                })
                                .map(|delta| {
                                    let width: f32 = delta.into();
                                    width.max(1.0)
                                })
                                .unwrap_or(fallback_width);
                            px(width)
                        }
                    };
                    let cursor_color = match cursor_style {
                        AppCursorStyle::Line => text_color,
                        AppCursorStyle::Block => selection_color,
                    };

                    Some(fill(
                        Bounds::new(
                            point(
                                line_bounds.left() + line_offset_x + cursor_x,
                                line_bounds.top(),
                            ),
                            size(cursor_width, line_bounds.size.height),
                        ),
                        cursor_color,
                    ))
                } else {
                    None
                };

                InlineInputPrepaintState {
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

                let line =
                    window.with_content_mask(Some(ContentMask { bounds: prepaint.line_bounds }), |window| {
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
                            .expect("failed to paint inline input text");
                            Some(line)
                        } else {
                            None
                        };

                        if let Some(cursor) = prepaint.cursor.take() {
                            window.paint_quad(cursor);
                        }

                        line
                    });

                view.update(cx, |this, _cx| {
                    if let Some(state) = this.active_inline_input_state_mut() {
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

impl TerminalView {
    fn inline_input_notify_target_for_target(target: InlineInputTarget) -> InlineInputNotifyTarget {
        match target {
            InlineInputTarget::CommandPalette => InlineInputNotifyTarget::Overlay,
            InlineInputTarget::RenameTab | InlineInputTarget::Search => {
                InlineInputNotifyTarget::Parent
            }
        }
    }

    fn notify_for_inline_input_target(
        &mut self,
        target: InlineInputTarget,
        cx: &mut Context<Self>,
    ) {
        match Self::inline_input_notify_target_for_target(target) {
            InlineInputNotifyTarget::Parent => cx.notify(),
            InlineInputNotifyTarget::Overlay => self.notify_overlay(cx),
        }
    }

    pub(super) fn notify_search_inline_input(&mut self, cx: &mut Context<Self>) {
        self.notify_for_inline_input_target(InlineInputTarget::Search, cx);
    }

    fn active_inline_input_target(&self) -> Option<InlineInputTarget> {
        if self.is_command_palette_open() {
            Some(InlineInputTarget::CommandPalette)
        } else if self.search_open {
            Some(InlineInputTarget::Search)
        } else if self.renaming_tab.is_some() {
            Some(InlineInputTarget::RenameTab)
        } else {
            None
        }
    }

    pub(super) fn has_active_inline_input(&self) -> bool {
        self.active_inline_input_target().is_some()
    }

    pub(super) fn render_inline_input_layer(
        &self,
        font: Font,
        font_size: Pixels,
        text_color: Hsla,
        selection_color: Hsla,
        alignment: InlineInputAlignment,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .cursor(gpui::CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(Self::handle_inline_input_mouse_down),
            )
            .on_mouse_move(cx.listener(Self::handle_inline_input_mouse_move))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(Self::handle_inline_input_mouse_up),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(Self::handle_inline_input_mouse_up),
            )
            .child(InlineInputElement::new(
                cx.entity(),
                self.focus_handle.clone(),
                font,
                font_size,
                text_color,
                selection_color,
                alignment,
            ))
            .into_any_element()
    }

    fn active_inline_input_state(&self) -> Option<&InlineInputState> {
        match self.active_inline_input_target()? {
            InlineInputTarget::CommandPalette => Some(self.command_palette_input()),
            InlineInputTarget::Search => Some(&self.search_input),
            InlineInputTarget::RenameTab => Some(&self.rename_input),
        }
    }

    fn active_inline_input_state_mut(&mut self) -> Option<&mut InlineInputState> {
        match self.active_inline_input_target()? {
            InlineInputTarget::CommandPalette => Some(self.command_palette_input_mut()),
            InlineInputTarget::Search => Some(&mut self.search_input),
            InlineInputTarget::RenameTab => Some(&mut self.rename_input),
        }
    }

    pub(super) fn command_palette_query_changed(&mut self, cx: &mut Context<Self>) {
        self.refresh_command_palette_matches(true, cx);
        self.notify_for_inline_input_target(InlineInputTarget::CommandPalette, cx);
    }

    fn enforce_tab_rename_limit(&mut self) {
        let current_chars = self.rename_input.text().chars().count();
        if current_chars <= MAX_TAB_TITLE_CHARS {
            return;
        }

        let truncated: String = self
            .rename_input
            .text()
            .chars()
            .take(MAX_TAB_TITLE_CHARS)
            .collect();
        self.rename_input.set_text(truncated);
    }

    fn apply_inline_input_mutation(
        &mut self,
        mutate: impl FnOnce(&mut InlineInputState),
        cx: &mut Context<Self>,
    ) {
        self.reset_cursor_blink_phase();

        match self.active_inline_input_target() {
            Some(InlineInputTarget::CommandPalette) => {
                mutate(self.command_palette_input_mut());
                self.command_palette_query_changed(cx);
            }
            Some(InlineInputTarget::Search) => {
                mutate(&mut self.search_input);
                self.handle_search_input_changed(cx);
            }
            Some(InlineInputTarget::RenameTab) => {
                mutate(&mut self.rename_input);
                self.enforce_tab_rename_limit();
                self.notify_for_inline_input_target(InlineInputTarget::RenameTab, cx);
            }
            None => {}
        }
    }

    pub(super) fn paste_clipboard_into_active_inline_input(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.has_active_inline_input() {
            return false;
        }

        let Some(clipboard_text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return true;
        };

        let filtered_text = filter_inline_paste_text(&clipboard_text);
        if filtered_text.is_empty() {
            return true;
        }

        self.apply_inline_input_mutation(
            move |state| state.replace_text_in_range(None, &filtered_text),
            cx,
        );
        true
    }

    pub(super) fn copy_active_inline_input_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(selected_text) = self
            .active_inline_input_state()
            .and_then(InlineInputState::selected_text)
        else {
            return self.has_active_inline_input();
        };

        cx.write_to_clipboard(ClipboardItem::new_string(selected_text));
        true
    }

    pub(super) fn handle_inline_input_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }

        self.focus_handle.focus(window, cx);

        let target_utf16 = match self.active_inline_input_state() {
            Some(state) => state.character_index_for_point(event.position),
            None => return,
        };

        self.apply_inline_input_mutation(
            |state| {
                if event.modifiers.shift {
                    state.select_to_utf16(target_utf16);
                } else if event.click_count >= 2 {
                    state.select_token_at_utf16(target_utf16);
                } else {
                    state.set_cursor_utf16(target_utf16);
                }
            },
            cx,
        );
        self.inline_input_selecting = true;
        cx.stop_propagation();
    }

    pub(super) fn handle_inline_input_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.inline_input_selecting || !event.dragging() {
            return;
        }

        let target_utf16 = match self.active_inline_input_state() {
            Some(state) => state.character_index_for_point(event.position),
            None => return,
        };

        self.apply_inline_input_mutation(|state| state.select_to_utf16(target_utf16), cx);
        cx.stop_propagation();
    }

    pub(super) fn handle_inline_input_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }

        self.inline_input_selecting = false;
        cx.stop_propagation();
    }

    pub(super) fn handle_inline_backspace_action(
        &mut self,
        _: &crate::commands::InlineBackspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_input_mutation(InlineInputState::delete_backward, cx);
    }

    pub(super) fn handle_inline_delete_action(
        &mut self,
        _: &crate::commands::InlineDelete,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_input_mutation(InlineInputState::delete_forward, cx);
    }

    pub(super) fn handle_inline_move_left_action(
        &mut self,
        _: &crate::commands::InlineMoveLeft,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_input_mutation(InlineInputState::move_left, cx);
    }

    pub(super) fn handle_inline_move_right_action(
        &mut self,
        _: &crate::commands::InlineMoveRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_input_mutation(InlineInputState::move_right, cx);
    }

    pub(super) fn handle_inline_select_left_action(
        &mut self,
        _: &crate::commands::InlineSelectLeft,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_input_mutation(InlineInputState::select_left, cx);
    }

    pub(super) fn handle_inline_select_right_action(
        &mut self,
        _: &crate::commands::InlineSelectRight,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_input_mutation(InlineInputState::select_right, cx);
    }

    pub(super) fn handle_inline_select_all_action(
        &mut self,
        _: &crate::commands::InlineSelectAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_input_mutation(InlineInputState::select_all, cx);
    }

    pub(super) fn handle_inline_move_to_start_action(
        &mut self,
        _: &crate::commands::InlineMoveToStart,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_input_mutation(InlineInputState::move_to_start, cx);
    }

    pub(super) fn handle_inline_move_to_end_action(
        &mut self,
        _: &crate::commands::InlineMoveToEnd,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_input_mutation(InlineInputState::move_to_end, cx);
    }

    pub(super) fn handle_inline_delete_word_backward_action(
        &mut self,
        _: &crate::commands::InlineDeleteWordBackward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_input_mutation(InlineInputState::delete_word_backward, cx);
    }

    pub(super) fn handle_inline_delete_word_forward_action(
        &mut self,
        _: &crate::commands::InlineDeleteWordForward,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_input_mutation(InlineInputState::delete_word_forward, cx);
    }

    pub(super) fn handle_inline_delete_to_start_action(
        &mut self,
        _: &crate::commands::InlineDeleteToStart,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_input_mutation(InlineInputState::delete_to_start, cx);
    }

    pub(super) fn handle_inline_delete_to_end_action(
        &mut self,
        _: &crate::commands::InlineDeleteToEnd,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_inline_input_mutation(InlineInputState::delete_to_end, cx);
    }
}

impl TerminalView {
    pub(super) fn ime_cursor_bounds(&self) -> Option<Bounds<Pixels>> {
        let geometry = self.terminal_viewport_geometry()?;
        let pane = self.active_pane_ref()?;
        let size = pane.terminal.size();
        let cell_width: f32 = size.cell_width.into();
        let cell_height: f32 = size.cell_height.into();
        // Use cursor_position() instead of cursor_state() so that IME
        // preedit is shown even when the TUI app hides the cursor.
        let (cursor_col, cursor_row) = pane.terminal.cursor_position();
        let x = geometry.origin_x + (cursor_col as f32) * cell_width;
        let y = geometry.origin_y + (cursor_row as f32) * cell_height;
        Some(Bounds::new(
            point(px(x), px(y)),
            gpui::size(px(cell_width), px(cell_height)),
        ))
    }
}

impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        if let Some(state) = self.active_inline_input_state() {
            return Some(state.text_for_range(range, adjusted_range));
        }
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if let Some(state) = self.active_inline_input_state() {
            return Some(state.selected_text_range());
        }
        Some(terminal_ime_selected_text_range(
            self.ime_selected_range.clone(),
        ))
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        if let Some(state) = self.active_inline_input_state() {
            return state.marked_text_range();
        }
        ime_marked_text_range_utf16(self.ime_marked_text.as_deref())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.has_active_inline_input() {
            self.apply_inline_input_mutation(InlineInputState::unmark_text, cx);
            return;
        }
        // Only clear marked text; do NOT commit to PTY.
        // Commit only happens in replace_text_in_range.
        self.ime_marked_text = None;
        self.ime_selected_range = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.has_active_inline_input() {
            self.apply_inline_input_mutation(
                move |state| state.replace_text_in_range(range, text),
                cx,
            );
            return;
        }
        // Terminal IME mode: confirmed text → send to PTY
        self.ime_marked_text = None;
        self.ime_selected_range = None;
        if !text.is_empty() {
            self.write_terminal_input(text.as_bytes(), cx);
        }
        self.clear_selection();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.has_active_inline_input() {
            self.apply_inline_input_mutation(
                move |state| {
                    state.replace_and_mark_text_in_range(range, new_text, new_selected_range)
                },
                cx,
            );
            return;
        }
        // Terminal IME mode: store composing text, do NOT send to PTY
        self.ime_marked_text = if new_text.is_empty() {
            None
        } else {
            Some(new_text.to_string())
        };
        self.ime_selected_range = new_selected_range;
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if let Some(state) = self.active_inline_input_state() {
            return Some(state.bounds_for_range(range_utf16, element_bounds));
        }
        // ime_cursor_bounds returns coordinates relative to the terminal
        // content area.  Offset by element_bounds.origin to convert to
        // window coordinates so macOS positions the candidate window correctly.
        let cursor = self.ime_cursor_bounds()?;
        let cell_width: f32 = self
            .active_pane_ref()
            .map(|pane| pane.terminal.size().cell_width.into())
            .unwrap_or_default();
        Some(ime_candidate_bounds(
            cursor,
            element_bounds,
            range_utf16.start,
            cell_width,
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        if let Some(state) = self.active_inline_input_state() {
            return Some(state.character_index_for_point(point));
        }
        None
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        true
    }
}

fn filter_inline_paste_text(text: &str) -> String {
    text.chars()
        .filter(|character| *character != '\n' && *character != '\r')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_range_conversion_handles_multibyte_text() {
        let state = InlineInputState::new("a😄é".to_string());
        let utf16 = state.range_to_utf16(&(1..7));
        assert_eq!(utf16, 1..4);
        let utf8 = state.range_from_utf16(&utf16);
        assert_eq!(utf8, 1..7);
    }

    #[test]
    fn replace_text_uses_selection_when_no_range() {
        let mut state = InlineInputState::new("hello".to_string());
        state.selected_range = 1..4;
        state.replace_text_in_range(None, "i");
        assert_eq!(state.text(), "hio");
        assert_eq!(state.selected_range(), 2..2);
    }

    #[test]
    fn selected_text_returns_none_without_selection() {
        let state = InlineInputState::new("hello".to_string());

        assert_eq!(state.selected_text(), None);
    }

    #[test]
    fn selected_text_returns_selected_inline_text() {
        let mut state = InlineInputState::new("hello world".to_string());
        state.selected_range = 0..5;

        assert_eq!(state.selected_text().as_deref(), Some("hello"));
    }

    #[test]
    fn replace_and_mark_sets_marked_and_selection() {
        let mut state = InlineInputState::new("abcd".to_string());
        state.selected_range = 1..3;
        state.replace_and_mark_text_in_range(Some(1..3), "xy", Some(0..1));
        assert_eq!(state.text(), "axyd");
        assert_eq!(state.marked_range, Some(1..3));
        assert_eq!(state.selected_range(), 1..2);
    }

    #[test]
    fn unmark_clears_marked_range() {
        let mut state = InlineInputState::new("abc".to_string());
        state.marked_range = Some(0..2);
        state.unmark_text();
        assert_eq!(state.marked_range, None);
    }

    #[test]
    fn delete_to_start_removes_prefix() {
        let mut state = InlineInputState::new("hello world".to_string());
        state.set_cursor_utf8(5);
        state.delete_to_start();
        assert_eq!(state.text(), " world");
        assert_eq!(state.selected_range(), 0..0);
    }

    #[test]
    fn delete_word_backward_removes_previous_word() {
        let mut state = InlineInputState::new("hello world".to_string());
        state.set_cursor_utf8(11);
        state.delete_word_backward();
        assert_eq!(state.text(), "hello ");
        assert_eq!(state.selected_range(), 6..6);
    }

    #[test]
    fn select_to_utf16_extends_selection() {
        let mut state = InlineInputState::new("a😄b".to_string());
        state.set_cursor_utf16(1);
        state.select_to_utf16(4);
        assert_eq!(state.selected_range(), 1..6);
    }

    #[test]
    fn select_token_at_utf16_selects_word_and_whitespace_runs() {
        let mut state = InlineInputState::new("hello  world".to_string());

        state.select_token_at_utf16(1);
        assert_eq!(state.selected_range(), 0..5);

        state.select_token_at_utf16(5);
        assert_eq!(state.selected_range(), 5..7);
    }

    #[test]
    fn select_token_at_utf16_handles_punctuation_and_end_of_line() {
        let mut state = InlineInputState::new("foo==bar".to_string());

        state.select_token_at_utf16(3);
        assert_eq!(state.selected_range(), 3..5);

        state.select_token_at_utf16(8);
        assert_eq!(state.selected_range(), 5..8);
    }

    #[test]
    fn inline_input_notify_policy_matches_overlay_split() {
        assert_eq!(
            TerminalView::inline_input_notify_target_for_target(InlineInputTarget::CommandPalette),
            InlineInputNotifyTarget::Overlay
        );
        assert_eq!(
            TerminalView::inline_input_notify_target_for_target(InlineInputTarget::Search),
            InlineInputNotifyTarget::Parent
        );
        assert_eq!(
            TerminalView::inline_input_notify_target_for_target(InlineInputTarget::RenameTab),
            InlineInputNotifyTarget::Parent
        );
    }

    #[test]
    fn inline_paste_filter_removes_line_breaks() {
        assert_eq!(
            filter_inline_paste_text("line-1\r\nline-2\nline-3\rline-4"),
            "line-1line-2line-3line-4"
        );
    }

    #[test]
    fn terminal_ime_marked_text_range_counts_utf16_units() {
        assert_eq!(ime_marked_text_range_utf16(None), None);
        assert_eq!(ime_marked_text_range_utf16(Some("a😄")), Some(0..3));
    }

    #[test]
    fn terminal_ime_selected_text_range_defaults_to_caret() {
        let empty = terminal_ime_selected_text_range(None);
        assert_eq!(empty.range, 0..0);
        assert!(!empty.reversed);

        let selected = terminal_ime_selected_text_range(Some(1..4));
        assert_eq!(selected.range, 1..4);
        assert!(!selected.reversed);
    }

    #[test]
    fn ime_candidate_bounds_offsets_cursor_into_window_space() {
        let cursor = Bounds::new(point(px(10.0), px(20.0)), size(px(8.0), px(16.0)));
        let element_bounds = Bounds::new(point(px(100.0), px(200.0)), size(px(320.0), px(240.0)));

        let bounds = ime_candidate_bounds(cursor, element_bounds, 2, 8.0);

        assert_eq!(bounds.origin, point(px(126.0), px(220.0)));
        assert_eq!(bounds.size, cursor.size);
    }
}
