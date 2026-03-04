use super::*;

pub(super) struct TerminalOverlayView {
    parent: WeakEntity<TerminalView>,
}

impl TerminalOverlayView {
    pub(super) fn new(parent: WeakEntity<TerminalView>) -> Self {
        Self { parent }
    }
}

impl Render for TerminalOverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.parent
            .update(cx, |view, cx| view.render_overlay_layer(window, cx))
            .unwrap_or_else(|_| div().id("terminal-overlay-empty").size_full().into_any_element())
    }
}
