//! Reporting an error that prevents Fulgur from starting at all.
//!
//! Such errors happen before the logger and the UI exist, and the Windows build
//! has no console, so the message goes to stderr and to a native dialog.

/// Tell the user that Fulgur cannot start.
///
/// ### Arguments
/// - `message`: The reason Fulgur cannot start, worded for the user
pub fn report_fatal_startup_error(message: &str) {
    eprintln!("Fulgur cannot start: {message}");
    show_native_error_dialog(message);
}

/// Show an error message box, on Windows.
///
/// ### Arguments
/// - `message`: The message to show
#[cfg(target_os = "windows")]
#[allow(unsafe_code, reason = "Win32 MessageBoxW shows the startup error")]
fn show_native_error_dialog(message: &str) {
    use windows::{
        Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
        core::{HSTRING, PCWSTR},
    };
    let text = HSTRING::from(message);
    let caption = HSTRING::from("Fulgur");
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// Show an error dialog through `zenity` or `kdialog`, on Linux.
///
/// ### Arguments
/// - `message`: The message to show
#[cfg(target_os = "linux")]
fn show_native_error_dialog(message: &str) {
    use std::process::Command;
    let zenity = [
        "--error",
        "--no-markup",
        "--title=Fulgur",
        "--text",
        message,
    ];
    let kdialog = ["--title", "Fulgur", "--error", message];
    let shown = Command::new("zenity").args(zenity).status().is_ok()
        || Command::new("kdialog").args(kdialog).status().is_ok();
    if !shown {
        eprintln!("No zenity or kdialog found to show the error in a dialog");
    }
}

/// No native dialog on other platforms: stderr is the only channel.
///
/// ### Arguments
/// - `_message`: The message, already written to stderr
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn show_native_error_dialog(_message: &str) {}
