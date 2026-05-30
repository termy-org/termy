use std::{error::Error, ffi::CString, fmt, os::raw::c_void};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use serde::Serialize;

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeTitlebarTabAction {
    Select = 1,
    New = 2,
}

pub type NativeTitlebarTabCallback =
    unsafe extern "C" fn(context: *mut c_void, action: i32, tab_id: *const i8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeAppKitError {
    WindowHandle,
    NonAppKitHandle,
    MissingView,
    UnsupportedPlatform,
    InvalidPayload,
    MissingWindow,
    BridgeFailed(i32),
}

impl fmt::Display for NativeAppKitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WindowHandle => write!(f, "failed to access the GPUI window handle"),
            Self::NonAppKitHandle => write!(f, "expected a macOS AppKit window handle"),
            Self::MissingView => write!(f, "AppKit window handle did not contain an NSView"),
            Self::UnsupportedPlatform => write!(f, "native AppKit titlebar tabs require macOS"),
            Self::InvalidPayload => write!(f, "failed to serialize native titlebar tabs payload"),
            Self::MissingWindow => write!(f, "NSView is not attached to an NSWindow"),
            Self::BridgeFailed(code) => {
                write!(f, "native AppKit SwiftUI bridge failed with code {code}")
            }
        }
    }
}

impl Error for NativeAppKitError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeTitlebarTab<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub is_selected: bool,
    pub is_pinned: bool,
    pub is_loading: bool,
}

impl<'a> NativeTitlebarTab<'a> {
    pub fn new(id: &'a str, title: &'a str) -> Self {
        Self {
            id,
            title,
            is_selected: false,
            is_pinned: false,
            is_loading: false,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.is_selected = selected;
        self
    }

    pub fn pinned(mut self, pinned: bool) -> Self {
        self.is_pinned = pinned;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.is_loading = loading;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NativeTitlebarTabsOptions<'a> {
    pub tabs: &'a [NativeTitlebarTab<'a>],
    pub selected_id: Option<&'a str>,
    pub height: f64,
    pub shows_add_button: bool,
    pub callback: Option<NativeTitlebarTabCallback>,
    pub callback_context: *mut c_void,
}

impl<'a> NativeTitlebarTabsOptions<'a> {
    pub fn new(tabs: &'a [NativeTitlebarTab<'a>]) -> Self {
        Self {
            tabs,
            selected_id: None,
            height: 30.0,
            shows_add_button: true,
            callback: None,
            callback_context: std::ptr::null_mut(),
        }
    }

    pub fn selected_id(mut self, selected_id: &'a str) -> Self {
        self.selected_id = Some(selected_id);
        self
    }

    pub fn height(mut self, height: f64) -> Self {
        self.height = height;
        self
    }

    pub fn shows_add_button(mut self, shows_add_button: bool) -> Self {
        self.shows_add_button = shows_add_button;
        self
    }

    pub fn callback(mut self, callback: NativeTitlebarTabCallback, context: *mut c_void) -> Self {
        self.callback = Some(callback);
        self.callback_context = context;
        self
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeTitlebarTabsPayload<'a> {
    tabs: Vec<NativeTitlebarTabPayload<'a>>,
    selected_id: Option<&'a str>,
    height: f64,
    shows_add_button: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeTitlebarTabPayload<'a> {
    id: &'a str,
    title: &'a str,
    is_selected: bool,
    is_pinned: bool,
    is_loading: bool,
}

pub fn install_or_update_titlebar_tabs(
    window: &gpui::Window,
    options: NativeTitlebarTabsOptions<'_>,
) -> Result<(), NativeAppKitError> {
    let ns_view = appkit_ns_view(window)?;
    let payload = payload_json(options)?;
    install_or_update_titlebar_tabs_for_ns_view(ns_view, &payload, options)
}

pub fn remove_titlebar_tabs(window: &gpui::Window) -> Result<(), NativeAppKitError> {
    let ns_view = appkit_ns_view(window)?;
    remove_titlebar_tabs_for_ns_view(ns_view)
}

fn appkit_ns_view(window: &gpui::Window) -> Result<*mut c_void, NativeAppKitError> {
    #[cfg(target_os = "macos")]
    {
        let handle =
            HasWindowHandle::window_handle(window).map_err(|_| NativeAppKitError::WindowHandle)?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err(NativeAppKitError::NonAppKitHandle);
        };
        let ns_view = handle.ns_view.as_ptr().cast::<c_void>();
        if ns_view.is_null() {
            Err(NativeAppKitError::MissingView)
        } else {
            Ok(ns_view)
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window;
        Err(NativeAppKitError::UnsupportedPlatform)
    }
}

fn payload_json(options: NativeTitlebarTabsOptions<'_>) -> Result<CString, NativeAppKitError> {
    let payload = NativeTitlebarTabsPayload {
        tabs: options
            .tabs
            .iter()
            .map(|tab| NativeTitlebarTabPayload {
                id: tab.id,
                title: tab.title,
                is_selected: tab.is_selected,
                is_pinned: tab.is_pinned,
                is_loading: tab.is_loading,
            })
            .collect(),
        selected_id: options.selected_id,
        height: options.height,
        shows_add_button: options.shows_add_button,
    };
    let payload = serde_json::to_string(&payload).map_err(|_| NativeAppKitError::InvalidPayload)?;
    CString::new(payload).map_err(|_| NativeAppKitError::InvalidPayload)
}

fn install_or_update_titlebar_tabs_for_ns_view(
    ns_view: *mut c_void,
    payload: &CString,
    options: NativeTitlebarTabsOptions<'_>,
) -> Result<(), NativeAppKitError> {
    #[cfg(target_os = "macos")]
    {
        let status = unsafe {
            gpui_native_appkit_install_or_update_titlebar_tabs(
                ns_view,
                payload.as_ptr(),
                options.callback,
                options.callback_context,
            )
        };
        bridge_status(status)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ns_view, payload, options);
        Err(NativeAppKitError::UnsupportedPlatform)
    }
}

fn remove_titlebar_tabs_for_ns_view(ns_view: *mut c_void) -> Result<(), NativeAppKitError> {
    #[cfg(target_os = "macos")]
    {
        let status = unsafe { gpui_native_appkit_remove_titlebar_tabs(ns_view) };
        bridge_status(status)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ns_view;
        Err(NativeAppKitError::UnsupportedPlatform)
    }
}

#[cfg(target_os = "macos")]
fn bridge_status(status: i32) -> Result<(), NativeAppKitError> {
    match status {
        0 => Ok(()),
        -1 => Err(NativeAppKitError::MissingView),
        -2 => Err(NativeAppKitError::MissingWindow),
        -3 => Err(NativeAppKitError::InvalidPayload),
        code => Err(NativeAppKitError::BridgeFailed(code)),
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn gpui_native_appkit_install_or_update_titlebar_tabs(
        ns_view: *mut c_void,
        payload_json: *const i8,
        callback: Option<NativeTitlebarTabCallback>,
        callback_context: *mut c_void,
    ) -> i32;

    fn gpui_native_appkit_remove_titlebar_tabs(ns_view: *mut c_void) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_uses_selected_id_and_tab_flags() {
        let tabs = [
            NativeTitlebarTab::new("one", "One").pinned(true),
            NativeTitlebarTab::new("two", "Two")
                .selected(true)
                .loading(true),
        ];
        let options = NativeTitlebarTabsOptions::new(&tabs)
            .selected_id("two")
            .height(32.0)
            .shows_add_button(false);

        let payload = payload_json(options).expect("payload should serialize");
        let payload = payload.to_str().expect("payload is utf-8");

        assert!(payload.contains(r#""selectedId":"two""#));
        assert!(payload.contains(r#""height":32.0"#));
        assert!(payload.contains(r#""showsAddButton":false"#));
        assert!(payload.contains(r#""isPinned":true"#));
        assert!(payload.contains(r#""isLoading":true"#));
    }
}
