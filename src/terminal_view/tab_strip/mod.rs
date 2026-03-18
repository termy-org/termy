pub(super) mod chrome;
pub(crate) mod constants;
pub(super) mod gestures;
pub(super) mod hints;
pub(super) mod hit_test;
pub(super) mod layout;
pub(super) mod render_controls;
pub(super) mod render_horizontal;
pub(super) mod render_palette;
pub(super) mod render_shared;
pub(super) mod render_tab_item;
pub(super) mod render_text_measure;
pub(super) mod render_vertical;
pub(super) mod state;
pub(super) mod titlebar_drag;

pub(crate) use self::layout::{
    clamp_expanded_vertical_tab_strip_width, collapsed_vertical_tab_strip_width,
    min_expanded_vertical_tab_strip_width,
};
