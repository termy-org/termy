use std::{error::Error, ffi::CString, fmt, os::raw::c_void};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

const TERMY_TABBING_IDENTIFIER: &str = "com.lassevestergaard.termy.terminal";

pub type NativeWindowTabCallback = unsafe extern "C" fn(context: *mut c_void);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeWindowHandle(*mut c_void);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeAppKitError {
    WindowHandle,
    NonAppKitHandle,
    MissingView,
    UnsupportedPlatform,
    InvalidString,
    MissingWindow,
    BridgeFailed(i32),
}

impl fmt::Display for NativeAppKitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WindowHandle => write!(f, "failed to access the GPUI window handle"),
            Self::NonAppKitHandle => write!(f, "expected a macOS AppKit window handle"),
            Self::MissingView => write!(f, "AppKit window handle did not contain an NSView"),
            Self::UnsupportedPlatform => write!(f, "native AppKit tabs require macOS"),
            Self::InvalidString => write!(f, "failed to pass native tab string to AppKit"),
            Self::MissingWindow => write!(f, "NSView is not attached to an NSWindow"),
            Self::BridgeFailed(code) => write!(f, "native AppKit bridge failed with code {code}"),
        }
    }
}

impl Error for NativeAppKitError {}

pub fn configure_window_tabbing(
    window: &gpui::Window,
    title: &str,
) -> Result<(), NativeAppKitError> {
    configure_window_tabbing_with_callback(window, title, None, std::ptr::null_mut())
}

pub fn configure_window_tabbing_with_callback(
    window: &gpui::Window,
    title: &str,
    callback: Option<NativeWindowTabCallback>,
    callback_context: *mut c_void,
) -> Result<(), NativeAppKitError> {
    let ns_view = appkit_ns_view(window)?;
    let identifier =
        CString::new(TERMY_TABBING_IDENTIFIER).map_err(|_| NativeAppKitError::InvalidString)?;
    let title = CString::new(title).map_err(|_| NativeAppKitError::InvalidString)?;
    configure_window_tabbing_for_ns_view(ns_view, &identifier, &title, callback, callback_context)
}

pub fn add_window_to_tab_group(
    anchor_window: &gpui::Window,
    window: &gpui::Window,
) -> Result<(), NativeAppKitError> {
    let anchor_handle = native_window_handle(anchor_window)?;
    let window_handle = native_window_handle(window)?;
    add_window_handles_to_tab_group(anchor_handle, window_handle)
}

pub fn native_window_handle(
    window: &gpui::Window,
) -> Result<NativeWindowHandle, NativeAppKitError> {
    appkit_ns_view(window).map(NativeWindowHandle)
}

pub fn add_window_handles_to_tab_group(
    anchor_window: NativeWindowHandle,
    window: NativeWindowHandle,
) -> Result<(), NativeAppKitError> {
    add_window_to_tab_group_for_ns_views(anchor_window.0, window.0)
}

/// Number of windows in this window's native AppKit tab group. Returns 0 when
/// the window is not part of a group (i.e. there is no native tab bar showing).
pub fn native_window_tab_group_count(window: &gpui::Window) -> Result<u32, NativeAppKitError> {
    let ns_view = appkit_ns_view(window)?;
    window_tab_group_count_for_ns_view(ns_view)
}

/// Update the title shown for this window in the native AppKit tab bar.
pub fn set_window_tab_title(window: &gpui::Window, title: &str) -> Result<(), NativeAppKitError> {
    let ns_view = appkit_ns_view(window)?;
    let title = CString::new(title).map_err(|_| NativeAppKitError::InvalidString)?;
    set_window_tab_title_for_ns_view(ns_view, &title)
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

fn configure_window_tabbing_for_ns_view(
    ns_view: *mut c_void,
    identifier: &CString,
    title: &CString,
    callback: Option<NativeWindowTabCallback>,
    callback_context: *mut c_void,
) -> Result<(), NativeAppKitError> {
    #[cfg(target_os = "macos")]
    {
        let status = unsafe {
            gpui_native_appkit_configure_window_tabbing(
                ns_view,
                identifier.as_ptr(),
                title.as_ptr(),
                callback,
                callback_context,
            )
        };
        bridge_status(status)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ns_view, identifier, title, callback, callback_context);
        Err(NativeAppKitError::UnsupportedPlatform)
    }
}

fn add_window_to_tab_group_for_ns_views(
    anchor_view: *mut c_void,
    window_view: *mut c_void,
) -> Result<(), NativeAppKitError> {
    #[cfg(target_os = "macos")]
    {
        let status =
            unsafe { gpui_native_appkit_add_window_to_tab_group(anchor_view, window_view) };
        bridge_status(status)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (anchor_view, window_view);
        Err(NativeAppKitError::UnsupportedPlatform)
    }
}

fn window_tab_group_count_for_ns_view(ns_view: *mut c_void) -> Result<u32, NativeAppKitError> {
    #[cfg(target_os = "macos")]
    {
        let count = unsafe { gpui_native_appkit_window_tab_group_count(ns_view) };
        if count < 0 {
            Err(NativeAppKitError::MissingWindow)
        } else {
            Ok(count as u32)
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ns_view;
        Err(NativeAppKitError::UnsupportedPlatform)
    }
}

fn set_window_tab_title_for_ns_view(
    ns_view: *mut c_void,
    title: &CString,
) -> Result<(), NativeAppKitError> {
    #[cfg(target_os = "macos")]
    {
        let status = unsafe { gpui_native_appkit_set_window_tab_title(ns_view, title.as_ptr()) };
        bridge_status(status)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ns_view, title);
        Err(NativeAppKitError::UnsupportedPlatform)
    }
}

#[cfg(target_os = "macos")]
fn bridge_status(status: i32) -> Result<(), NativeAppKitError> {
    match status {
        0 => Ok(()),
        -1 => Err(NativeAppKitError::MissingView),
        -2 => Err(NativeAppKitError::MissingWindow),
        -3 => Err(NativeAppKitError::InvalidString),
        code => Err(NativeAppKitError::BridgeFailed(code)),
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn gpui_native_appkit_configure_window_tabbing(
        ns_view: *mut c_void,
        identifier: *const i8,
        title: *const i8,
        callback: Option<NativeWindowTabCallback>,
        callback_context: *mut c_void,
    ) -> i32;

    fn gpui_native_appkit_add_window_to_tab_group(
        anchor_view: *mut c_void,
        window_view: *mut c_void,
    ) -> i32;

    fn gpui_native_appkit_window_tab_group_count(ns_view: *mut c_void) -> i32;

    fn gpui_native_appkit_set_window_tab_title(ns_view: *mut c_void, title: *const i8) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabbing_identifier_is_stable() {
        assert_eq!(
            CString::new(TERMY_TABBING_IDENTIFIER)
                .expect("identifier should be C-compatible")
                .to_str()
                .expect("identifier should be UTF-8"),
            "com.lassevestergaard.termy.terminal"
        );
    }
}
