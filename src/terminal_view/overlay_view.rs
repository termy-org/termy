use super::*;

pub(super) struct TerminalOverlayView {
    parent: WeakEntity<TerminalView>,
    parent_missing_warned: bool,
}

impl TerminalOverlayView {
    pub(super) fn new(parent: WeakEntity<TerminalView>) -> Self {
        Self {
            parent,
            parent_missing_warned: false,
        }
    }
}

impl Render for TerminalOverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Ok(layer) = self
            .parent
            .update(cx, |view, cx| view.render_overlay_layer(window, cx))
        {
            layer
        } else {
            if !self.parent_missing_warned {
                self.parent_missing_warned = true;
                log::warn!("Terminal overlay render skipped because parent view is unavailable");
            }
            // Parent teardown can race with child repaint; keep this non-panicking.
            div()
                .id("terminal-overlay-empty")
                .size_full()
                .into_any_element()
        }
    }
}
