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
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetForegroundWindow,
    GetWindowThreadProcessId, IDYES, MB_ICONINFORMATION, MB_OK, MB_YESNO, MF_GRAYED, MF_SEPARATOR,
    MF_STRING, MessageBoxW, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabContextMenuAction {
    Rename,
    Pin,
    Unpin,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentProjectContextMenuAction {
    NewSession,
    RenameProject,
    ToggleGitPanel,
    Pin,
    Unpin,
    RevealProject,
    CopyPath,
    CollapseProject,
    ExpandProject,
    DeleteProject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentThreadContextMenuAction {
    RestartSession,
    CloseSession,
    RenameThread,
    ToggleGitPanel,
    Pin,
    Unpin,
    DeleteThread,
}

const CONTEXT_MENU_COPY_ID: i32 = 1;
const CONTEXT_MENU_PASTE_ID: i32 = 2;
const CONTEXT_MENU_OPEN_SEARCH_ID: i32 = 3;
const CONTEXT_MENU_COPY_BUFFER_POSITION_ID: i32 = 4;
const TAB_CONTEXT_MENU_PIN_ID: i32 = 101;
const TAB_CONTEXT_MENU_UNPIN_ID: i32 = 102;
const TAB_CONTEXT_MENU_RENAME_ID: i32 = 103;
const TAB_CONTEXT_MENU_CLOSE_ID: i32 = 104;
const AGENT_PROJECT_CONTEXT_MENU_NEW_SESSION_ID: i32 = 201;
const AGENT_PROJECT_CONTEXT_MENU_RENAME_ID: i32 = 202;
const AGENT_PROJECT_CONTEXT_MENU_TOGGLE_GIT_PANEL_ID: i32 = 203;
const AGENT_PROJECT_CONTEXT_MENU_PIN_ID: i32 = 204;
const AGENT_PROJECT_CONTEXT_MENU_UNPIN_ID: i32 = 205;
const AGENT_PROJECT_CONTEXT_MENU_REVEAL_ID: i32 = 206;
const AGENT_PROJECT_CONTEXT_MENU_COPY_PATH_ID: i32 = 207;
const AGENT_PROJECT_CONTEXT_MENU_COLLAPSE_ID: i32 = 208;
const AGENT_PROJECT_CONTEXT_MENU_EXPAND_ID: i32 = 209;
const AGENT_PROJECT_CONTEXT_MENU_DELETE_ID: i32 = 210;
const AGENT_THREAD_CONTEXT_MENU_RESTART_SESSION_ID: i32 = 301;
const AGENT_THREAD_CONTEXT_MENU_CLOSE_SESSION_ID: i32 = 302;
const AGENT_THREAD_CONTEXT_MENU_RENAME_ID: i32 = 303;
const AGENT_THREAD_CONTEXT_MENU_TOGGLE_GIT_PANEL_ID: i32 = 304;
const AGENT_THREAD_CONTEXT_MENU_PIN_ID: i32 = 305;
const AGENT_THREAD_CONTEXT_MENU_UNPIN_ID: i32 = 306;
const AGENT_THREAD_CONTEXT_MENU_DELETE_ID: i32 = 307;

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
) -> Option<ContextMenuAction> {
    #[cfg(target_os = "macos")]
    {
        fn show_copy_paste_context_menu_on_main(
            mtm: MainThreadMarker,
            buffer_position_label: Option<String>,
            can_copy: bool,
            can_paste: bool,
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
            let open_search_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Open Search",
                CONTEXT_MENU_OPEN_SEARCH_ID,
                true,
            );

            menu.addItem(&copy_item);
            menu.addItem(&paste_item);
            menu.addItem(&open_search_item);
            menu.addItem(&copy_buffer_position_item);

            CONTEXT_MENU_SELECTION.store(0, Ordering::Relaxed);
            let location: NSPoint = NSEvent::mouseLocation();
            let _ = menu.popUpMenuPositioningItem_atLocation_inView(None, location, None);

            match CONTEXT_MENU_SELECTION.swap(0, Ordering::Relaxed) {
                CONTEXT_MENU_COPY_ID => Some(ContextMenuAction::Copy),
                CONTEXT_MENU_PASTE_ID => Some(ContextMenuAction::Paste),
                CONTEXT_MENU_OPEN_SEARCH_ID => Some(ContextMenuAction::OpenSearch),
                CONTEXT_MENU_COPY_BUFFER_POSITION_ID => Some(ContextMenuAction::CopyBufferPosition),
                _ => None,
            }
        }

        if let Some(mtm) = MainThreadMarker::new() {
            return show_copy_paste_context_menu_on_main(
                mtm,
                buffer_position_label,
                can_copy,
                can_paste,
            );
        }
        return run_on_main(|mtm| {
            show_copy_paste_context_menu_on_main(mtm, buffer_position_label, can_copy, can_paste)
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
        let copy_buffer_position_flags = if has_buffer_position {
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
            _ => None,
        };
    }

    #[cfg(any(
        target_os = "linux",
        not(any(target_os = "macos", target_os = "windows"))
    ))]
    {
        let _ = (buffer_position_label, can_copy, can_paste);
        None
    }
}

pub fn show_tab_context_menu(pinned: bool) -> Option<TabContextMenuAction> {
    #[cfg(target_os = "macos")]
    {
        fn show_tab_context_menu_on_main(
            mtm: MainThreadMarker,
            pinned: bool,
        ) -> Option<TabContextMenuAction> {
            let app = NSApplication::sharedApplication(mtm);
            let Some(_current_event) = app.currentEvent() else {
                return None;
            };

            let menu = NSMenu::new(mtm);
            menu.setAutoenablesItems(false);

            // Rename Tab
            let rename_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Rename Tab",
                TAB_CONTEXT_MENU_RENAME_ID,
                true,
            );
            menu.addItem(&rename_item);

            // Pin/Unpin Tab
            let (pin_title, pin_action_id) = if pinned {
                ("Unpin Tab", TAB_CONTEXT_MENU_UNPIN_ID)
            } else {
                ("Pin Tab", TAB_CONTEXT_MENU_PIN_ID)
            };
            let pin_item =
                TermyContextMenuItem::new_with_action_id(mtm, pin_title, pin_action_id, true);
            menu.addItem(&pin_item);

            // Separator
            menu.addItem(&NSMenuItem::separatorItem(mtm));

            // Close Tab
            let close_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Close Tab",
                TAB_CONTEXT_MENU_CLOSE_ID,
                true,
            );
            menu.addItem(&close_item);

            CONTEXT_MENU_SELECTION.store(0, Ordering::Relaxed);
            let location: NSPoint = NSEvent::mouseLocation();
            let _ = menu.popUpMenuPositioningItem_atLocation_inView(None, location, None);

            match CONTEXT_MENU_SELECTION.swap(0, Ordering::Relaxed) {
                TAB_CONTEXT_MENU_RENAME_ID => Some(TabContextMenuAction::Rename),
                TAB_CONTEXT_MENU_PIN_ID => Some(TabContextMenuAction::Pin),
                TAB_CONTEXT_MENU_UNPIN_ID => Some(TabContextMenuAction::Unpin),
                TAB_CONTEXT_MENU_CLOSE_ID => Some(TabContextMenuAction::Close),
                _ => None,
            }
        }

        if let Some(mtm) = MainThreadMarker::new() {
            return show_tab_context_menu_on_main(mtm, pinned);
        }
        return run_on_main(|mtm| show_tab_context_menu_on_main(mtm, pinned));
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

        // Rename Tab
        let rename_title = wide_string("Rename Tab");
        unsafe {
            AppendMenuW(
                menu,
                MF_STRING,
                TAB_CONTEXT_MENU_RENAME_ID as usize,
                windows::core::PCWSTR(rename_title.as_ptr()),
            )
            .ok()?;
        }

        // Pin/Unpin Tab
        let (pin_title, pin_action_id) = if pinned {
            (wide_string("Unpin Tab"), TAB_CONTEXT_MENU_UNPIN_ID)
        } else {
            (wide_string("Pin Tab"), TAB_CONTEXT_MENU_PIN_ID)
        };
        unsafe {
            AppendMenuW(
                menu,
                MF_STRING,
                pin_action_id as usize,
                windows::core::PCWSTR(pin_title.as_ptr()),
            )
            .ok()?;
        }

        // Separator
        unsafe {
            AppendMenuW(menu, MF_SEPARATOR, 0, windows::core::PCWSTR::null()).ok()?;
        }

        // Close Tab
        let close_title = wide_string("Close Tab");
        unsafe {
            AppendMenuW(
                menu,
                MF_STRING,
                TAB_CONTEXT_MENU_CLOSE_ID as usize,
                windows::core::PCWSTR(close_title.as_ptr()),
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
            TAB_CONTEXT_MENU_RENAME_ID => Some(TabContextMenuAction::Rename),
            TAB_CONTEXT_MENU_PIN_ID => Some(TabContextMenuAction::Pin),
            TAB_CONTEXT_MENU_UNPIN_ID => Some(TabContextMenuAction::Unpin),
            TAB_CONTEXT_MENU_CLOSE_ID => Some(TabContextMenuAction::Close),
            _ => None,
        };
    }

    #[cfg(any(
        target_os = "linux",
        not(any(target_os = "macos", target_os = "windows"))
    ))]
    {
        let _ = pinned;
        None
    }
}

pub fn show_agent_project_context_menu(
    pinned: bool,
    collapsed: bool,
    git_panel_visible: bool,
) -> Option<AgentProjectContextMenuAction> {
    #[cfg(target_os = "macos")]
    {
        fn show_agent_project_context_menu_on_main(
            mtm: MainThreadMarker,
            pinned: bool,
            collapsed: bool,
            git_panel_visible: bool,
        ) -> Option<AgentProjectContextMenuAction> {
            let app = NSApplication::sharedApplication(mtm);
            let Some(_current_event) = app.currentEvent() else {
                return None;
            };

            let menu = NSMenu::new(mtm);
            menu.setAutoenablesItems(false);

            let new_session_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "New Session",
                AGENT_PROJECT_CONTEXT_MENU_NEW_SESSION_ID,
                true,
            );
            let rename_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Rename Project",
                AGENT_PROJECT_CONTEXT_MENU_RENAME_ID,
                true,
            );
            let git_panel_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                if git_panel_visible {
                    "Hide Git Changes"
                } else {
                    "Show Git Changes"
                },
                AGENT_PROJECT_CONTEXT_MENU_TOGGLE_GIT_PANEL_ID,
                true,
            );
            let pin_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                if pinned {
                    "Unpin Project"
                } else {
                    "Pin Project"
                },
                if pinned {
                    AGENT_PROJECT_CONTEXT_MENU_UNPIN_ID
                } else {
                    AGENT_PROJECT_CONTEXT_MENU_PIN_ID
                },
                true,
            );
            let reveal_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Reveal Project",
                AGENT_PROJECT_CONTEXT_MENU_REVEAL_ID,
                true,
            );
            let copy_path_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Copy Project Path",
                AGENT_PROJECT_CONTEXT_MENU_COPY_PATH_ID,
                true,
            );
            let collapse_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                if collapsed {
                    "Expand Project"
                } else {
                    "Collapse Project"
                },
                if collapsed {
                    AGENT_PROJECT_CONTEXT_MENU_EXPAND_ID
                } else {
                    AGENT_PROJECT_CONTEXT_MENU_COLLAPSE_ID
                },
                true,
            );
            let delete_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Delete Project",
                AGENT_PROJECT_CONTEXT_MENU_DELETE_ID,
                true,
            );
            menu.addItem(&new_session_item);
            menu.addItem(&rename_item);
            menu.addItem(&git_panel_item);
            menu.addItem(&pin_item);
            menu.addItem(&reveal_item);
            menu.addItem(&copy_path_item);
            menu.addItem(&collapse_item);
            menu.addItem(&delete_item);

            CONTEXT_MENU_SELECTION.store(0, Ordering::Relaxed);
            let location: NSPoint = NSEvent::mouseLocation();
            let _ = menu.popUpMenuPositioningItem_atLocation_inView(None, location, None);

            match CONTEXT_MENU_SELECTION.swap(0, Ordering::Relaxed) {
                AGENT_PROJECT_CONTEXT_MENU_NEW_SESSION_ID => {
                    Some(AgentProjectContextMenuAction::NewSession)
                }
                AGENT_PROJECT_CONTEXT_MENU_RENAME_ID => {
                    Some(AgentProjectContextMenuAction::RenameProject)
                }
                AGENT_PROJECT_CONTEXT_MENU_TOGGLE_GIT_PANEL_ID => {
                    Some(AgentProjectContextMenuAction::ToggleGitPanel)
                }
                AGENT_PROJECT_CONTEXT_MENU_PIN_ID => Some(AgentProjectContextMenuAction::Pin),
                AGENT_PROJECT_CONTEXT_MENU_UNPIN_ID => Some(AgentProjectContextMenuAction::Unpin),
                AGENT_PROJECT_CONTEXT_MENU_REVEAL_ID => {
                    Some(AgentProjectContextMenuAction::RevealProject)
                }
                AGENT_PROJECT_CONTEXT_MENU_COPY_PATH_ID => {
                    Some(AgentProjectContextMenuAction::CopyPath)
                }
                AGENT_PROJECT_CONTEXT_MENU_COLLAPSE_ID => {
                    Some(AgentProjectContextMenuAction::CollapseProject)
                }
                AGENT_PROJECT_CONTEXT_MENU_EXPAND_ID => {
                    Some(AgentProjectContextMenuAction::ExpandProject)
                }
                AGENT_PROJECT_CONTEXT_MENU_DELETE_ID => {
                    Some(AgentProjectContextMenuAction::DeleteProject)
                }
                _ => None,
            }
        }

        if let Some(mtm) = MainThreadMarker::new() {
            return show_agent_project_context_menu_on_main(
                mtm,
                pinned,
                collapsed,
                git_panel_visible,
            );
        }
        return run_on_main(move |mtm| {
            show_agent_project_context_menu_on_main(mtm, pinned, collapsed, git_panel_visible)
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

        let new_session = wide_string("New Session");
        let rename_project = wide_string("Rename Project");
        let toggle_git_panel = wide_string(if git_panel_visible {
            "Hide Git Changes"
        } else {
            "Show Git Changes"
        });
        let pin_project = wide_string(if pinned {
            "Unpin Project"
        } else {
            "Pin Project"
        });
        let reveal_project = wide_string("Reveal Project");
        let copy_project_path = wide_string("Copy Project Path");
        let collapse_project = wide_string(if collapsed {
            "Expand Project"
        } else {
            "Collapse Project"
        });
        let delete_project = wide_string("Delete Project");

        unsafe {
            AppendMenuW(
                menu,
                MF_STRING,
                AGENT_PROJECT_CONTEXT_MENU_NEW_SESSION_ID as usize,
                windows::core::PCWSTR(new_session.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                AGENT_PROJECT_CONTEXT_MENU_RENAME_ID as usize,
                windows::core::PCWSTR(rename_project.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                AGENT_PROJECT_CONTEXT_MENU_TOGGLE_GIT_PANEL_ID as usize,
                windows::core::PCWSTR(toggle_git_panel.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                if pinned {
                    AGENT_PROJECT_CONTEXT_MENU_UNPIN_ID as usize
                } else {
                    AGENT_PROJECT_CONTEXT_MENU_PIN_ID as usize
                },
                windows::core::PCWSTR(pin_project.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                AGENT_PROJECT_CONTEXT_MENU_REVEAL_ID as usize,
                windows::core::PCWSTR(reveal_project.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                AGENT_PROJECT_CONTEXT_MENU_COPY_PATH_ID as usize,
                windows::core::PCWSTR(copy_project_path.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                if collapsed {
                    AGENT_PROJECT_CONTEXT_MENU_EXPAND_ID as usize
                } else {
                    AGENT_PROJECT_CONTEXT_MENU_COLLAPSE_ID as usize
                },
                windows::core::PCWSTR(collapse_project.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                AGENT_PROJECT_CONTEXT_MENU_DELETE_ID as usize,
                windows::core::PCWSTR(delete_project.as_ptr()),
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
            AGENT_PROJECT_CONTEXT_MENU_NEW_SESSION_ID => {
                Some(AgentProjectContextMenuAction::NewSession)
            }
            AGENT_PROJECT_CONTEXT_MENU_RENAME_ID => {
                Some(AgentProjectContextMenuAction::RenameProject)
            }
            AGENT_PROJECT_CONTEXT_MENU_TOGGLE_GIT_PANEL_ID => {
                Some(AgentProjectContextMenuAction::ToggleGitPanel)
            }
            AGENT_PROJECT_CONTEXT_MENU_PIN_ID => Some(AgentProjectContextMenuAction::Pin),
            AGENT_PROJECT_CONTEXT_MENU_UNPIN_ID => Some(AgentProjectContextMenuAction::Unpin),
            AGENT_PROJECT_CONTEXT_MENU_REVEAL_ID => {
                Some(AgentProjectContextMenuAction::RevealProject)
            }
            AGENT_PROJECT_CONTEXT_MENU_COPY_PATH_ID => {
                Some(AgentProjectContextMenuAction::CopyPath)
            }
            AGENT_PROJECT_CONTEXT_MENU_COLLAPSE_ID => {
                Some(AgentProjectContextMenuAction::CollapseProject)
            }
            AGENT_PROJECT_CONTEXT_MENU_EXPAND_ID => {
                Some(AgentProjectContextMenuAction::ExpandProject)
            }
            AGENT_PROJECT_CONTEXT_MENU_DELETE_ID => {
                Some(AgentProjectContextMenuAction::DeleteProject)
            }
            _ => None,
        };
    }

    #[cfg(any(
        target_os = "linux",
        not(any(target_os = "macos", target_os = "windows"))
    ))]
    {
        let _ = (pinned, collapsed, git_panel_visible);
        None
    }
}

pub fn show_agent_thread_context_menu(
    has_live_session: bool,
    pinned: bool,
    git_panel_visible: bool,
) -> Option<AgentThreadContextMenuAction> {
    #[cfg(target_os = "macos")]
    {
        fn show_agent_thread_context_menu_on_main(
            mtm: MainThreadMarker,
            has_live_session: bool,
            pinned: bool,
            git_panel_visible: bool,
        ) -> Option<AgentThreadContextMenuAction> {
            let app = NSApplication::sharedApplication(mtm);
            let Some(_current_event) = app.currentEvent() else {
                return None;
            };

            let menu = NSMenu::new(mtm);
            menu.setAutoenablesItems(false);

            let restart_session_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                if has_live_session {
                    "Restart Session"
                } else {
                    "Open Session"
                },
                AGENT_THREAD_CONTEXT_MENU_RESTART_SESSION_ID,
                true,
            );
            let close_session_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Close Session",
                AGENT_THREAD_CONTEXT_MENU_CLOSE_SESSION_ID,
                has_live_session,
            );
            let rename_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Rename Thread",
                AGENT_THREAD_CONTEXT_MENU_RENAME_ID,
                true,
            );
            let git_panel_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                if git_panel_visible {
                    "Hide Git Changes"
                } else {
                    "Show Git Changes"
                },
                AGENT_THREAD_CONTEXT_MENU_TOGGLE_GIT_PANEL_ID,
                true,
            );
            let pin_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                if pinned { "Unpin Thread" } else { "Pin Thread" },
                if pinned {
                    AGENT_THREAD_CONTEXT_MENU_UNPIN_ID
                } else {
                    AGENT_THREAD_CONTEXT_MENU_PIN_ID
                },
                true,
            );
            let delete_item = TermyContextMenuItem::new_with_action_id(
                mtm,
                "Delete Thread",
                AGENT_THREAD_CONTEXT_MENU_DELETE_ID,
                true,
            );
            menu.addItem(&restart_session_item);
            menu.addItem(&close_session_item);
            menu.addItem(&rename_item);
            menu.addItem(&git_panel_item);
            menu.addItem(&pin_item);
            menu.addItem(&delete_item);

            CONTEXT_MENU_SELECTION.store(0, Ordering::Relaxed);
            let location: NSPoint = NSEvent::mouseLocation();
            let _ = menu.popUpMenuPositioningItem_atLocation_inView(None, location, None);

            match CONTEXT_MENU_SELECTION.swap(0, Ordering::Relaxed) {
                AGENT_THREAD_CONTEXT_MENU_RESTART_SESSION_ID => {
                    Some(AgentThreadContextMenuAction::RestartSession)
                }
                AGENT_THREAD_CONTEXT_MENU_CLOSE_SESSION_ID => {
                    Some(AgentThreadContextMenuAction::CloseSession)
                }
                AGENT_THREAD_CONTEXT_MENU_RENAME_ID => {
                    Some(AgentThreadContextMenuAction::RenameThread)
                }
                AGENT_THREAD_CONTEXT_MENU_TOGGLE_GIT_PANEL_ID => {
                    Some(AgentThreadContextMenuAction::ToggleGitPanel)
                }
                AGENT_THREAD_CONTEXT_MENU_PIN_ID => Some(AgentThreadContextMenuAction::Pin),
                AGENT_THREAD_CONTEXT_MENU_UNPIN_ID => Some(AgentThreadContextMenuAction::Unpin),
                AGENT_THREAD_CONTEXT_MENU_DELETE_ID => {
                    Some(AgentThreadContextMenuAction::DeleteThread)
                }
                _ => None,
            }
        }

        if let Some(mtm) = MainThreadMarker::new() {
            return show_agent_thread_context_menu_on_main(
                mtm,
                has_live_session,
                pinned,
                git_panel_visible,
            );
        }
        return run_on_main(move |mtm| {
            show_agent_thread_context_menu_on_main(mtm, has_live_session, pinned, git_panel_visible)
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

        let restart_session = wide_string(if has_live_session {
            "Restart Session"
        } else {
            "Open Session"
        });
        let close_session = wide_string("Close Session");
        let rename_thread = wide_string("Rename Thread");
        let toggle_git_panel = wide_string(if git_panel_visible {
            "Hide Git Changes"
        } else {
            "Show Git Changes"
        });
        let pin_thread = wide_string(if pinned { "Unpin Thread" } else { "Pin Thread" });
        let delete_thread = wide_string("Delete Thread");

        unsafe {
            AppendMenuW(
                menu,
                if has_live_session {
                    MF_STRING
                } else {
                    MF_STRING
                },
                AGENT_THREAD_CONTEXT_MENU_RESTART_SESSION_ID as usize,
                windows::core::PCWSTR(restart_session.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                if has_live_session {
                    MF_STRING
                } else {
                    MF_STRING | MF_GRAYED
                },
                AGENT_THREAD_CONTEXT_MENU_CLOSE_SESSION_ID as usize,
                windows::core::PCWSTR(close_session.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                AGENT_THREAD_CONTEXT_MENU_RENAME_ID as usize,
                windows::core::PCWSTR(rename_thread.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                AGENT_THREAD_CONTEXT_MENU_TOGGLE_GIT_PANEL_ID as usize,
                windows::core::PCWSTR(toggle_git_panel.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                if pinned {
                    AGENT_THREAD_CONTEXT_MENU_UNPIN_ID as usize
                } else {
                    AGENT_THREAD_CONTEXT_MENU_PIN_ID as usize
                },
                windows::core::PCWSTR(pin_thread.as_ptr()),
            )
            .ok()?;
            AppendMenuW(
                menu,
                MF_STRING,
                AGENT_THREAD_CONTEXT_MENU_DELETE_ID as usize,
                windows::core::PCWSTR(delete_thread.as_ptr()),
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
            AGENT_THREAD_CONTEXT_MENU_RESTART_SESSION_ID => {
                Some(AgentThreadContextMenuAction::RestartSession)
            }
            AGENT_THREAD_CONTEXT_MENU_CLOSE_SESSION_ID => {
                Some(AgentThreadContextMenuAction::CloseSession)
            }
            AGENT_THREAD_CONTEXT_MENU_RENAME_ID => Some(AgentThreadContextMenuAction::RenameThread),
            AGENT_THREAD_CONTEXT_MENU_TOGGLE_GIT_PANEL_ID => {
                Some(AgentThreadContextMenuAction::ToggleGitPanel)
            }
            AGENT_THREAD_CONTEXT_MENU_PIN_ID => Some(AgentThreadContextMenuAction::Pin),
            AGENT_THREAD_CONTEXT_MENU_UNPIN_ID => Some(AgentThreadContextMenuAction::Unpin),
            AGENT_THREAD_CONTEXT_MENU_DELETE_ID => Some(AgentThreadContextMenuAction::DeleteThread),
            _ => None,
        };
    }

    #[cfg(any(
        target_os = "linux",
        not(any(target_os = "macos", target_os = "windows"))
    ))]
    {
        let _ = (has_live_session, pinned, git_panel_visible);
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

/// Show a desktop notification.
///
/// This is a best-effort operation - failures are silently ignored.
/// On macOS, uses AppleScript/osascript for maximum compatibility.
/// On Windows, uses PowerShell toast notifications.
/// On Linux, uses notify-send or kdialog.
pub fn show_notification(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        // Use osascript for reliable notifications on macOS
        // This works without requiring notification permissions setup
        let script = format!(
            r#"display notification "{}" with title "{}""#,
            escape_applescript(body),
            escape_applescript(title)
        );
        let _ = Command::new("osascript").args(["-e", &script]).output();
    }

    #[cfg(target_os = "linux")]
    {
        if has_command("notify-send") {
            let _ = Command::new("notify-send").args([title, body]).output();
        } else if has_command("kdialog") {
            let _ = Command::new("kdialog")
                .args(["--passivepopup", body, "5", "--title", title])
                .output();
        }
    }

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        // Use PowerShell for Windows toast notifications
        let script = format!(
            r#"[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
$template = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02)
$textNodes = $template.GetElementsByTagName("text")
$textNodes.Item(0).AppendChild($template.CreateTextNode("{}")) | Out-Null
$textNodes.Item(1).AppendChild($template.CreateTextNode("{}")) | Out-Null
$toast = [Windows.UI.Notifications.ToastNotification]::new($template)
$notifier = [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier("Termy")
$notifier.Show($toast)"#,
            escape_powershell(title),
            escape_powershell(body)
        );
        let _ = Command::new("powershell")
            .args(["-WindowStyle", "Hidden", "-Command", &script])
            .output();
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = (title, body);
    }
}

/// Check if the application is currently the active/focused application.
///
/// Returns `true` if the app is focused, `false` otherwise.
/// On unsupported platforms, returns `true` (assume focused).
pub fn is_app_active() -> bool {
    #[cfg(target_os = "macos")]
    {
        run_on_main(|mtm| {
            let app = NSApplication::sharedApplication(mtm);
            app.isActive()
        })
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows, check if our process owns the foreground window
        unsafe {
            let foreground = GetForegroundWindow();
            if foreground.is_invalid() {
                return false;
            }
            let mut foreground_pid: u32 = 0;
            GetWindowThreadProcessId(foreground, Some(&mut foreground_pid));
            foreground_pid == std::process::id()
        }
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, there's no simple way to check focus without X11/Wayland bindings
        // Return true as a safe default
        true
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        true
    }
}

#[cfg(target_os = "macos")]
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "windows")]
fn escape_powershell(s: &str) -> String {
    s.replace('`', "``").replace('"', "`\"").replace('$', "`$")
}
