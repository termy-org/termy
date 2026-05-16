use cocoa::base::{BOOL, NO, id, nil};
use gpui::Window;
use objc::{
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{fmt, sync::OnceLock};

static NON_DRAGGABLE_CONTENT_VIEW_CLASS: OnceLock<usize> = OnceLock::new();

unsafe extern "C" {
    fn object_getClass(obj: *mut Object) -> *const Class;
    fn object_setClass(obj: *mut Object, cls: *const Class) -> *const Class;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeTitlebarDragError {
    WindowHandle,
    NonAppKitHandle,
    MissingView,
    MissingWindow,
    MissingViewClass,
    ClassRegistration,
}

impl fmt::Display for NativeTitlebarDragError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WindowHandle => write!(f, "Failed to access the macOS window handle."),
            Self::NonAppKitHandle => write!(
                f,
                "macOS titlebar drag bridge requires an AppKit window handle.",
            ),
            Self::MissingView => write!(f, "macOS titlebar drag bridge requires a live NSView."),
            Self::MissingWindow => {
                write!(f, "macOS titlebar drag bridge requires a live NSWindow.")
            }
            Self::MissingViewClass => write!(
                f,
                "macOS titlebar drag bridge could not read the NSView class."
            ),
            Self::ClassRegistration => write!(
                f,
                "macOS titlebar drag bridge failed to register its NSView subclass.",
            ),
        }
    }
}

pub(crate) fn disable_automatic_content_view_window_drag(
    window: &Window,
) -> Result<(), NativeTitlebarDragError> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|_| NativeTitlebarDragError::WindowHandle)?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(NativeTitlebarDragError::NonAppKitHandle);
    };

    let ns_view = handle.ns_view.as_ptr().cast::<Object>();
    if ns_view.is_null() {
        return Err(NativeTitlebarDragError::MissingView);
    }

    unsafe { disable_automatic_content_view_window_drag_for_view(ns_view) }
}

unsafe fn disable_automatic_content_view_window_drag_for_view(
    ns_view: *mut Object,
) -> Result<(), NativeTitlebarDragError> {
    let ns_window: id = unsafe { msg_send![ns_view, window] };
    if ns_window == nil {
        return Err(NativeTitlebarDragError::MissingWindow);
    }

    unsafe {
        let _: () = msg_send![ns_window, setMovableByWindowBackground: NO];
    }

    let current_class = unsafe { object_getClass(ns_view) };
    if current_class.is_null() {
        return Err(NativeTitlebarDragError::MissingViewClass);
    }

    let non_draggable_class = non_draggable_content_view_class(current_class)?;
    if current_class != non_draggable_class {
        unsafe {
            object_setClass(ns_view, non_draggable_class);
        }
    }

    Ok(())
}

fn non_draggable_content_view_class(
    superclass: *const Class,
) -> Result<*const Class, NativeTitlebarDragError> {
    let superclass =
        unsafe { superclass.as_ref() }.ok_or(NativeTitlebarDragError::MissingViewClass)?;
    let class = *NON_DRAGGABLE_CONTENT_VIEW_CLASS.get_or_init(|| unsafe {
        let Some(mut decl) = ClassDecl::new("TermyNonDraggableGPUIView", superclass) else {
            return 0;
        };
        decl.add_method(
            sel!(mouseDownCanMoveWindow),
            mouse_down_can_move_window as extern "C" fn(&Object, Sel) -> BOOL,
        );
        std::ptr::from_ref::<Class>(decl.register()) as usize
    });

    if class == 0 {
        Err(NativeTitlebarDragError::ClassRegistration)
    } else {
        Ok(class as *const Class)
    }
}

extern "C" fn mouse_down_can_move_window(_this: &Object, _sel: Sel) -> BOOL {
    NO
}
