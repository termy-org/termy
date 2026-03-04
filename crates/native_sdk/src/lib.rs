#[cfg(target_os = "macos")]
use dispatch2::run_on_main;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSAlert, NSAlertSecondButtonReturn};
#[cfg(target_os = "macos")]
use objc2_foundation::NSString;

#[cfg(target_os = "linux")]
use std::process::Command;

#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    IDYES, MB_ICONINFORMATION, MB_OK, MB_YESNO, MessageBoxW,
};

#[cfg(target_os = "windows")]
fn wide_string(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
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
