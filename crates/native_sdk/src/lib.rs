#[cfg(target_os = "macos")]
use dispatch2::run_on_main;
#[cfg(target_os = "macos")]
use objc2::{
    DeclaredClass, MainThreadOnly, define_class, msg_send, rc::Retained, runtime::AnyObject, sel,
};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSAlert, NSAlertSecondButtonReturn, NSApplication, NSEvent, NSImage, NSMenu, NSMenuItem,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{MainThreadMarker, NSData, NSPoint, NSString};
#[cfg(target_os = "macos")]
use std::cell::Cell;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicI32, Ordering};

#[cfg(target_os = "linux")]
use std::process::Command;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, POINT};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetForegroundWindow, IDYES,
    MB_ICONINFORMATION, MB_OK, MB_YESNO, MF_GRAYED, MF_STRING, MessageBoxW, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
};

#[cfg(target_os = "windows")]
fn wide_string(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextMenuAction {
    Copy,
    Paste,
    OpenSearch,
    CopyBufferPosition,
    AskAi,
    SearchGoogle,
}

const CONTEXT_MENU_COPY_ID: i32 = 1;
const CONTEXT_MENU_PASTE_ID: i32 = 2;
const CONTEXT_MENU_OPEN_SEARCH_ID: i32 = 3;
const CONTEXT_MENU_COPY_BUFFER_POSITION_ID: i32 = 4;
const CONTEXT_MENU_ASK_AI_ID: i32 = 5;
const CONTEXT_MENU_SEARCH_GOOGLE_ID: i32 = 6;

#[cfg(target_os = "macos")]
static CONTEXT_MENU_SELECTION: AtomicI32 = AtomicI32::new(0);

#[cfg(target_os = "macos")]
define_class!(
    #[unsafe(super(NSMenuItem))]
    #[name = "TermyContextMenuItem"]
    #[thread_kind = MainThreadOnly]
    #[ivars = Cell<i32>]
    struct TermyContextMenuItem;

    impl TermyContextMenuItem {
        #[unsafe(method(fireContextMenuAction:))]
        fn fire_context_menu_action(&self, _sender: Option<&AnyObject>) {
            CONTEXT_MENU_SELECTION.store(self.ivars().get(), Ordering::Relaxed);
        }
    }
);

#[cfg(target_os = "macos")]
impl TermyContextMenuItem {
    fn new_with_action_id(
        mtm: MainThreadMarker,
        title: &str,
        action_id: i32,
        enabled: bool,
    ) -> Retained<Self> {
        let this = mtm.alloc().set_ivars(Cell::new(action_id));
        let title = NSString::from_str(title);
        let key_equivalent = NSString::from_str("");
        let item: Retained<Self> = unsafe {
            msg_send![
                super(this),
                initWithTitle: &*title,
                action: Some(sel!(fireContextMenuAction:)),
                keyEquivalent: &*key_equivalent
            ]
        };
        unsafe {
            item.setTarget(Some(&item));
        }
        item.setEnabled(enabled);
        item
    }
}

#[cfg(target_os = "linux")]
fn has_command(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Set the macOS dock icon from PNG data. No-op on other platforms.
/// Set the macOS dock icon from PNG data. No-op on other platforms.
///
/// # Safety
/// Must be called from the main thread (e.g. inside `application.run()`).
pub fn set_dock_icon_from_png(png_data: &[u8]) {
    #[cfg(target_os = "macos")]
    {
        use objc2::AnyThread;

        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let data = NSData::with_bytes(png_data);
        if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
            let app = NSApplication::sharedApplication(mtm);
            unsafe {
                app.setApplicationIconImage(Some(&image));
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = png_data;
    }
}

pub fn show_alert(title: &str, message: &str) {
    #[cfg(target_os = "macos")]
    {
        run_on_main(|mtm| {
            let alert = NSAlert::new(mtm);
            let ns_title = NSString::from_str(title);
            let ns_message = NSString::from_str(message);
            let ok = NSString::from_str("OK");

            alert.setMessageText(&ns_title);
            alert.setInformativeText(&ns_message);
            let _ = alert.addButtonWithTitle(&ok);
            let _ = alert.runModal();
        });
    }

    #[cfg(target_os = "linux")]
    {
        if has_command("zenity") {
            let _ = Command::new("zenity")
                .args(["--info", "--title", title, "--text", message])
                .status();
        } else if has_command("kdialog") {
            let _ = Command::new("kdialog")
                .args(["--msgbox", message, "--title", title])
                .status();
        } else {
            eprintln!("[native_sdk] show_alert: {title}: {message}");
        }
    }

    #[cfg(target_os = "windows")]
    {
        let wide_title = wide_string(title);
        let wide_message = wide_string(message);
        unsafe {
            MessageBoxW(
                None,
                windows::core::PCWSTR(wide_message.as_ptr()),
                windows::core::PCWSTR(wide_title.as_ptr()),
                MB_OK | MB_ICONINFORMATION,
            );
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        eprintln!("[native_sdk] show_alert: {title}: {message}");
    }
}

pub fn show_copy_paste_context_menu(
    buffer_position_label: Option<String>,
    can_copy: bool,
    can_paste: bool,
    can_ask_ai: bool,
    can_search_google: bool,
) -> Option<ContextMenuAction> {
    #[cfg(target_os = "macos")]
    {
        fn show_copy_paste_context_menu_on_main(
            mtm: MainThreadMarker,
            buffer_position_label: Option<String>,
            can_copy: bool,
            can_paste: bool,
            can_ask_ai: bool,
            can_search_google: bool,
        ) -> Option<ContextMenuAction> {
            let app = NSApplication::sharedApplication(mtm);
            let Some(_current_event) = app.currentEvent() else {
                return None;
            };

            let menu = NSMenu::new(mtm);
            menu.setAutoenablesItems(false);

            if let Some(buffer_position_label) = buffer_position_label.as_ref() {
                let buffer_position_item =
                    TermyContextMenuItem::new_with_action_id(mtm, buffer_position_label, 0, false);
                menu.addItem(&buffer_position_item);
            }

            let copy_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Copy",
                CONTEXT_MENU_COPY_ID,
                can_copy,
            );
            let paste_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Paste",
                CONTEXT_MENU_PASTE_ID,
                can_paste,
            );
            let copy_buffer_position_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Copy Buffer Position",
                CONTEXT_MENU_COPY_BUFFER_POSITION_ID,
                buffer_position_label.is_some(),
            );
            let ask_ai_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Ask AI",
                CONTEXT_MENU_ASK_AI_ID,
                can_ask_ai,
            );
            let open_search_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Open Search",
                CONTEXT_MENU_OPEN_SEARCH_ID,
                true,
            );
            let search_google_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Search Google",
                CONTEXT_MENU_SEARCH_GOOGLE_ID,
                can_search_google,
            );

            menu.addItem(&copy_item);
            menu.addItem(&paste_item);
            menu.addItem(&open_search_item);
            menu.addItem(&copy_buffer_position_item);
            menu.addItem(&ask_ai_item);
            menu.addItem(&search_google_item);

            CONTEXT_MENU_SELECTION.store(0, Ordering::Relaxed);
            let location: NSPoint = NSEvent::mouseLocation();
            let _ = menu.popUpMenuPositioningItem_atLocation_inView(None, location, None);

            match CONTEXT_MENU_SELECTION.swap(0, Ordering::Relaxed) {
                CONTEXT_MENU_COPY_ID => Some(ContextMenuAction::Copy),
                CONTEXT_MENU_PASTE_ID => Some(ContextMenuAction::Paste),
                CONTEXT_MENU_OPEN_SEARCH_ID => Some(ContextMenuAction::OpenSearch),
                CONTEXT_MENU_COPY_BUFFER_POSITION_ID => Some(ContextMenuAction::CopyBufferPosition),
                CONTEXT_MENU_ASK_AI_ID => Some(ContextMenuAction::AskAi),
                CONTEXT_MENU_SEARCH_GOOGLE_ID => Some(ContextMenuAction::SearchGoogle),
                _ => None,
            }
        }

        if let Some(mtm) = MainThreadMarker::new() {
            return show_copy_paste_context_menu_on_main(
                mtm,
                buffer_position_label,
                can_copy,
                can_paste,
                can_ask_ai,
                can_search_google,
            );
        }
        return run_on_main(|mtm| {
            show_copy_paste_context_menu_on_main(
                mtm,
                buffer_position_label,
                can_copy,
                can_paste,
                can_ask_ai,
                can_search_google,
            )
        });
    }

    #[cfg(target_os = "windows")]
    {
        let menu = unsafe { CreatePopupMenu().ok()? };
        struct MenuGuard(windows::Win32::UI::WindowsAndMessaging::HMENU);
        impl Drop for MenuGuard {
            fn drop(&mut self) {
                let _ = unsafe { DestroyMenu(self.0) };
            }
        }
        let _menu_guard = MenuGuard(menu);

        let has_buffer_position = buffer_position_label.is_some();
        if let Some(buffer_position_label) = buffer_position_label.as_ref() {
            let buffer_position_title = wide_string(buffer_position_label);
            unsafe {
                AppendMenuW(
                    menu,
                    MF_STRING | MF_GRAYED,
                    0,
                    windows::core::PCWSTR(buffer_position_title.as_ptr()),
                )
                .ok()?;
            }
        }

        let copy_title = wide_string("Copy");
        let paste_title = wide_string("Paste");
        let open_search_title = wide_string("Open Search");
        let copy_buffer_position_title = wide_string("Copy Buffer Position");
        let ask_ai_title = wide_string("Ask AI");
        let search_google_title = wide_string("Search Google");
        let copy_flags = if can_copy {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        let paste_flags = if can_paste {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        let ask_ai_flags = if can_ask_ai {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        let copy_buffer_position_flags = if has_buffer_position {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        let search_google_flags = if can_search_google {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };

        unsafe {
            AppendMenuW(
                menu,
                copy_flags,
                CONTEXT_MENU_COPY_ID as usize,
                windows::core::PCWSTR(copy_title.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                paste_flags,
                CONTEXT_MENU_PASTE_ID as usize,
                windows::core::PCWSTR(paste_title.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                CONTEXT_MENU_OPEN_SEARCH_ID as usize,
                windows::core::PCWSTR(open_search_title.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                copy_buffer_position_flags,
                CONTEXT_MENU_COPY_BUFFER_POSITION_ID as usize,
                windows::core::PCWSTR(copy_buffer_position_title.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                ask_ai_flags,
                CONTEXT_MENU_ASK_AI_ID as usize,
                windows::core::PCWSTR(ask_ai_title.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                search_google_flags,
                CONTEXT_MENU_SEARCH_GOOGLE_ID as usize,
                windows::core::PCWSTR(search_google_title.as_ptr()),
            )
            .ok()?;
        }

        let mut cursor = POINT::default();
        unsafe {
            GetCursorPos(&mut cursor).ok()?;
        }
        let owner: HWND = unsafe { GetForegroundWindow() };
        let result = unsafe {
            TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
                cursor.x,
                cursor.y,
                Some(0),
                owner,
                None,
            )
            .0
        };

        return match result {
            CONTEXT_MENU_COPY_ID => Some(ContextMenuAction::Copy),
            CONTEXT_MENU_PASTE_ID => Some(ContextMenuAction::Paste),
            CONTEXT_MENU_OPEN_SEARCH_ID => Some(ContextMenuAction::OpenSearch),
            CONTEXT_MENU_COPY_BUFFER_POSITION_ID => Some(ContextMenuAction::CopyBufferPosition),
            CONTEXT_MENU_ASK_AI_ID => Some(ContextMenuAction::AskAi),
            CONTEXT_MENU_SEARCH_GOOGLE_ID => Some(ContextMenuAction::SearchGoogle),
            _ => None,
        };
    }

    #[cfg(any(
        target_os = "linux",
        not(any(target_os = "macos", target_os = "windows"))
    ))]
    {
        let _ = (
            buffer_position_label,
            can_copy,
            can_paste,
            can_ask_ai,
            can_search_google,
        );
        None
    }
}

pub fn confirm(title: &str, message: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        run_on_main(|mtm| {
            let alert = NSAlert::new(mtm);
            let ns_title = NSString::from_str(title);
            let ns_message = NSString::from_str(message);
            let cancel = NSString::from_str("Cancel");
            let ok = NSString::from_str("OK");

            alert.setMessageText(&ns_title);
            alert.setInformativeText(&ns_message);
            let _ = alert.addButtonWithTitle(&cancel);
            let _ = alert.addButtonWithTitle(&ok);

            let response = alert.runModal();
            response == NSAlertSecondButtonReturn
        })
    }

    #[cfg(target_os = "linux")]
    {
        if has_command("zenity") {
            Command::new("zenity")
                .args(["--question", "--title", title, "--text", message])
                .status()
                .is_ok_and(|s| s.success())
        } else if has_command("kdialog") {
            Command::new("kdialog")
                .args(["--yesno", message, "--title", title])
                .status()
                .is_ok_and(|s| s.success())
        } else {
            eprintln!("[native_sdk] confirm: {title}: {message}");
            false
        }
    }

    #[cfg(target_os = "windows")]
    {
        let wide_title = wide_string(title);
        let wide_message = wide_string(message);
        let result = unsafe {
            MessageBoxW(
                None,
                windows::core::PCWSTR(wide_message.as_ptr()),
                windows::core::PCWSTR(wide_title.as_ptr()),
                MB_YESNO | MB_ICONINFORMATION,
            )
        };
        result == IDYES
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        eprintln!("[native_sdk] confirm: {title}: {message}");
        false
    }
}
