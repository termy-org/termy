//! gpui-component scrollbar wrapper that maintains compatibility with Termy's scrollbar interface

use gpui::{AnyElement, InteractiveElement, IntoElement, ParentElement, Point, Pixels, Rgba, ScrollHandle, Size, Styled, div, px};
use gpui_component::scroll::{Scrollbar, ScrollbarHandle};
use std::rc::Rc;
use std::sync::Arc;

/// Wrapper that adapts gpui-component Scrollbar to Termy's scrollbar interface
pub struct GpuiComponentScrollbar {
    inner: Scrollbar,
}

impl GpuiComponentScrollbar {
    /// Create a new gpui-component scrollbar wrapper
    pub fn new(scroll_handle: impl ScrollbarHandle + Clone + 'static) -> Self {
        let scrollbar = Scrollbar::vertical(&scroll_handle);

        Self { inner: scrollbar }
    }



    /// Get the inner gpui-component scrollbar for rendering
    pub fn inner(&self) -> &Scrollbar {
        &self.inner
    }

    /// Create a scrollbar with Termy's styling
    pub fn with_termy_styling(self, style: super::scrollbar::ScrollbarPaintStyle) -> Self {
        // TODO: Apply Termy-specific styling to the gpui-component scrollbar
        // This would involve setting colors, sizes, etc. to match Termy's design
        self
    }
}

/// Adapter to make Termy's scroll handles compatible with gpui-component
pub struct TermyScrollHandleAdapter {
    termy_handle: ScrollHandle,
}

impl TermyScrollHandleAdapter {
    pub fn new(termy_handle: ScrollHandle) -> Self {
        Self { termy_handle }
    }
}

impl ScrollbarHandle for TermyScrollHandleAdapter {
    fn offset(&self) -> Point<Pixels> {
        self.termy_handle.offset()
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.termy_handle.set_offset(offset);
    }

    fn content_size(&self) -> Size<Pixels> {
        self.termy_handle.content_size()
    }
}