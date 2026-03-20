use cocoa::{
    appkit::{
        NSFilenamesPboardType, NSPasteboard, NSPasteboardItem, NSURLPboardType, NSView,
        NSViewHeightSizable, NSViewWidthSizable,
    },
    base::{BOOL, NO, YES, id, nil},
    foundation::{NSArray, NSAutoreleasePool, NSFastEnumeration, NSPoint, NSString, NSUInteger},
};
use flume::Sender;
use gpui::Window;
use objc::{
    class,
    declare::ClassDecl,
    msg_send,
    runtime::{Class, Object, Sel},
    sel, sel_impl,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    ffi::{CStr, c_void},
    fmt,
    path::PathBuf,
    ptr,
    sync::OnceLock,
};

const OVERLAY_STATE_IVAR: &str = "termyNativeFileDropState";
const NS_DRAG_OPERATION_NONE: NSUInteger = 0;
const NS_DRAG_OPERATION_COPY: NSUInteger = 1;
static OVERLAY_CLASS: OnceLock<usize> = OnceLock::new();

pub(crate) type NativeDropResult = Result<Vec<PathBuf>, NativeDropError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NativeDropError {
    Install(&'static str),
    UnsupportedDrag,
    InvalidUtf8,
    InvalidFileUrl(String),
}

impl fmt::Display for NativeDropError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Install(message) => write!(f, "{message}"),
            Self::UnsupportedDrag => write!(f, "Only Finder file drops are supported here."),
            Self::InvalidUtf8 => write!(f, "Finder drop data was not valid UTF-8."),
            Self::InvalidFileUrl(url) => {
                write!(f, "Finder drop did not contain a valid file URL: {url}")
            }
        }
    }
}

struct NativeDropState {
    sender: Sender<NativeDropResult>,
}

pub(crate) fn install_native_file_drop(
    window: &Window,
    sender: Sender<NativeDropResult>,
) -> Result<(), NativeDropError> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|_| NativeDropError::Install("Failed to access the macOS window handle."))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(NativeDropError::Install(
            "macOS file drop bridge requires an AppKit window handle.",
        ));
    };

    let ns_view = handle.ns_view.as_ptr().cast::<Object>();
    if ns_view.is_null() {
        return Err(NativeDropError::Install(
            "macOS file drop bridge requires a live NSView.",
        ));
    }

    unsafe { install_overlay(ns_view, sender) }
}

unsafe fn install_overlay(
    ns_view: *mut Object,
    sender: Sender<NativeDropResult>,
) -> Result<(), NativeDropError> {
    let class = overlay_class();
    let bounds = unsafe { NSView::bounds(ns_view as id) };
    let overlay: id = unsafe { msg_send![class, alloc] };
    let overlay: id = unsafe { NSView::initWithFrame_(overlay, bounds) };
    if overlay == nil {
        return Err(NativeDropError::Install(
            "Failed to create the macOS file drop overlay view.",
        ));
    }

    let state = Box::into_raw(Box::new(NativeDropState { sender })) as *mut c_void;
    unsafe {
        (&mut *overlay).set_ivar(OVERLAY_STATE_IVAR, state);
    }

    unsafe {
        NSView::setAutoresizingMask_(overlay, NSViewWidthSizable | NSViewHeightSizable);
    }

    let public_file_url = unsafe {
        NSString::alloc(nil)
            .init_str("public.file-url")
            .autorelease()
    };
    let drag_types =
        unsafe { NSArray::arrayWithObjects(nil, &[NSFilenamesPboardType, public_file_url]) };
    unsafe {
        let _: () = msg_send![overlay, registerForDraggedTypes: drag_types];
        NSView::addSubview_(ns_view as id, overlay);
    }
    Ok(())
}

fn overlay_class() -> *const Class {
    OVERLAY_CLASS
        .get_or_init(|| {
            let mut decl = ClassDecl::new("TermyNativeFileDropOverlayView", class!(NSView))
                .expect("overlay class should register once");
            decl.add_ivar::<*mut c_void>(OVERLAY_STATE_IVAR);

            unsafe {
                decl.add_method(sel!(dealloc), dealloc as extern "C" fn(&mut Object, Sel));
                decl.add_method(
                    sel!(hitTest:),
                    hit_test as extern "C" fn(&Object, Sel, NSPoint) -> id,
                );
                decl.add_method(
                    sel!(draggingEntered:),
                    dragging_entered as extern "C" fn(&Object, Sel, id) -> NSUInteger,
                );
                decl.add_method(
                    sel!(draggingUpdated:),
                    dragging_updated as extern "C" fn(&Object, Sel, id) -> NSUInteger,
                );
                decl.add_method(
                    sel!(draggingExited:),
                    dragging_exited as extern "C" fn(&Object, Sel, id),
                );
                decl.add_method(
                    sel!(performDragOperation:),
                    perform_drag_operation as extern "C" fn(&Object, Sel, id) -> BOOL,
                );
            }

            decl.register() as *const Class as usize
        })
        .to_owned() as *const Class
}

extern "C" fn dealloc(this: &mut Object, _sel: Sel) {
    unsafe {
        let state_ptr = *this.get_ivar::<*mut c_void>(OVERLAY_STATE_IVAR);
        if !state_ptr.is_null() {
            drop(Box::from_raw(state_ptr.cast::<NativeDropState>()));
            this.set_ivar(OVERLAY_STATE_IVAR, ptr::null_mut::<c_void>());
        }

        let _: () = msg_send![super(this, class!(NSView)), dealloc];
    }
}

extern "C" fn hit_test(_this: &Object, _sel: Sel, _point: NSPoint) -> id {
    nil
}

extern "C" fn dragging_entered(this: &Object, _sel: Sel, dragging_info: id) -> NSUInteger {
    drag_operation_for_info(this, dragging_info)
}

extern "C" fn dragging_updated(this: &Object, _sel: Sel, dragging_info: id) -> NSUInteger {
    drag_operation_for_info(this, dragging_info)
}

extern "C" fn dragging_exited(_this: &Object, _sel: Sel, _dragging_info: id) {}

extern "C" fn perform_drag_operation(this: &Object, _sel: Sel, dragging_info: id) -> BOOL {
    let result = decode_dragged_paths(dragging_info);
    let accepted = result.is_ok();
    if unsafe { state(this) }.sender.send(result).is_err() {
        return NO;
    }

    if accepted { YES } else { NO }
}

fn drag_operation_for_info(_this: &Object, dragging_info: id) -> NSUInteger {
    if decode_dragged_paths(dragging_info).is_ok() {
        NS_DRAG_OPERATION_COPY
    } else {
        NS_DRAG_OPERATION_NONE
    }
}

unsafe fn state(this: &Object) -> &NativeDropState {
    let state_ptr = unsafe { *this.get_ivar::<*mut c_void>(OVERLAY_STATE_IVAR) };
    unsafe { &*state_ptr.cast::<NativeDropState>() }
}

fn decode_dragged_paths(dragging_info: id) -> NativeDropResult {
    let pasteboard: id = unsafe { msg_send![dragging_info, draggingPasteboard] };

    let file_url_paths = decode_file_url_items(pasteboard)?;
    if let Some(paths) = file_url_paths {
        return Ok(paths);
    }

    let legacy_paths = decode_legacy_filename_list(pasteboard)?;
    if !legacy_paths.is_empty() {
        return Ok(legacy_paths);
    }

    Err(NativeDropError::UnsupportedDrag)
}

fn decode_file_url_items(pasteboard: id) -> Result<Option<Vec<PathBuf>>, NativeDropError> {
    let items = unsafe { NSPasteboard::pasteboardItems(pasteboard) };
    if items == nil {
        return Ok(None);
    }

    let public_file_url = unsafe {
        NSString::alloc(nil)
            .init_str("public.file-url")
            .autorelease()
    };
    let mut paths = Vec::new();
    let mut saw_file_url = false;

    for item in unsafe { items.iter() } {
        let file_url = unsafe { NSPasteboardItem::stringForType(item, public_file_url) };
        if file_url == nil {
            continue;
        }

        saw_file_url = true;
        paths.push(resolve_file_url_path(file_url)?);
    }

    if saw_file_url {
        if paths.is_empty() {
            return Err(NativeDropError::UnsupportedDrag);
        }
        return Ok(Some(paths));
    }

    let legacy_url = unsafe { NSPasteboard::stringForType(pasteboard, NSURLPboardType) };
    if legacy_url != nil {
        return Ok(Some(vec![resolve_file_url_path(legacy_url)?]));
    }

    Ok(None)
}

fn resolve_file_url_path(file_url: id) -> Result<PathBuf, NativeDropError> {
    let url_string = nsstring_to_string(file_url)?;
    let ns_url: id = unsafe { msg_send![class!(NSURL), URLWithString: file_url] };
    if ns_url == nil {
        return Err(NativeDropError::InvalidFileUrl(url_string));
    }

    let is_file_url: BOOL = unsafe { msg_send![ns_url, isFileURL] };
    if is_file_url != YES {
        return Err(NativeDropError::InvalidFileUrl(url_string));
    }

    let file_path_url = unsafe { normalize_file_url(ns_url) };
    if file_path_url == nil {
        return Err(NativeDropError::InvalidFileUrl(url_string));
    }

    let path: id = unsafe { msg_send![file_path_url, path] };
    if path == nil {
        return Err(NativeDropError::InvalidFileUrl(url_string));
    }

    Ok(PathBuf::from(nsstring_to_string(path)?))
}

unsafe fn normalize_file_url(ns_url: id) -> id {
    let is_reference: BOOL = unsafe { msg_send![ns_url, isFileReferenceURL] };
    let mut resolved = if is_reference == YES {
        let file_path_url: id = unsafe { msg_send![ns_url, filePathURL] };
        if file_path_url == nil {
            return nil;
        }
        file_path_url
    } else {
        ns_url
    };

    let standardized: id = unsafe { msg_send![resolved, URLByStandardizingPath] };
    if standardized != nil {
        resolved = standardized;
    }

    let symlink_resolved: id = unsafe { msg_send![resolved, URLByResolvingSymlinksInPath] };
    if symlink_resolved != nil {
        resolved = symlink_resolved;
    }

    resolved
}

fn decode_legacy_filename_list(pasteboard: id) -> Result<Vec<PathBuf>, NativeDropError> {
    let filenames = unsafe { NSPasteboard::propertyListForType(pasteboard, NSFilenamesPboardType) };
    if filenames == nil {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for file in unsafe { filenames.iter() } {
        paths.push(PathBuf::from(nsstring_to_string(file)?));
    }
    Ok(paths)
}

fn nsstring_to_string(value: id) -> Result<String, NativeDropError> {
    let utf8 = unsafe { NSString::UTF8String(value) };
    if utf8.is_null() {
        return Err(NativeDropError::InvalidUtf8);
    }

    Ok(unsafe { CStr::from_ptr(utf8) }
        .to_str()
        .map_err(|_| NativeDropError::InvalidUtf8)?
        .to_owned())
}
