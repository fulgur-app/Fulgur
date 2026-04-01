//! Windows taskbar Jump List implementation.
#![allow(unsafe_op_in_unsafe_fn)]
//!
//! Mirrors the macOS dock menu: shows a "Recent Files" category, an "Open Tabs"
//! category for file-backed tabs across all open windows, and a Tasks section
//! with "New Tab" and "New Window" entries that launch a fresh Fulgur instance.

use std::path::PathBuf;
use windows::{
    Win32::{
        Foundation::PROPERTYKEY,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemAlloc,
                StructuredStorage::{PROPVARIANT, PropVariantClear},
            },
            Variant::{VARENUM, VT_LPWSTR},
        },
        UI::Shell::{
            Common::{IObjectArray, IObjectCollection},
            DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW,
            PropertiesSystem::IPropertyStore,
            SetCurrentProcessExplicitAppUserModelID, ShellLink,
        },
    },
    core::{GUID, HSTRING, Interface, PCWSTR, PWSTR},
};

use crate::fulgur::ui::menus::DockMenuTab;

/// PKEY_Title: {F29F85E0-4FF9-1068-AB91-08002B27B3D9}, PID 2
/// Sets the visible label of a jump-list item.
const PKEY_TITLE: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xf29f85e0_4ff9_1068_ab91_08002b27b3d9),
    pid: 2,
};

/// Set the process-wide Application User Model ID
///
/// Must be called once at the very start of `main()`, before any window is
/// created, so that the taskbar button and the jump list share the same AUMID.
/// Without this, `ICustomDestinationList::SetAppID` registers a jump list under
/// an ID that the taskbar button never queries, making the list invisible.
pub fn set_app_user_model_id() {
    unsafe {
        if let Err(e) = SetCurrentProcessExplicitAppUserModelID(PCWSTR(
            HSTRING::from("app.fulgur.fulgur").as_ptr(),
        )) {
            log::warn!("Failed to set AppUserModelID: {e}");
        }
    }
}

/// Build and apply the Windows taskbar Jump List
///
/// Delegates to the unsafe COM implementation. Any error is logged and
/// swallowed — a broken jump list must never crash the app.
///
/// ### Arguments
/// - `windows_data`: Tab lists for every open window; only file-backed tabs are shown
/// - `recent_files`: Ordered list of recently opened file paths
pub fn update_windows_jump_list(windows_data: &[Vec<DockMenuTab>], recent_files: &[PathBuf]) {
    if let Err(e) = unsafe { try_update_jump_list(windows_data, recent_files) } {
        log::warn!("Failed to update Windows jump list: {e}");
    }
}

/// Build and commit the jump list via the Windows COM API
///
/// Creates the `ICustomDestinationList`, appends the Tasks section ("New Tab",
/// "New Window"), the "Recent Files" category, and the "Open Tabs" category
/// (file-backed tabs only), then commits the list.
///
/// ### Arguments
/// - `windows_data`: Tab lists for every open window; only file-backed tabs are shown
/// - `recent_files`: Ordered list of recently opened file paths
///
/// ### Returns
/// - `Ok(())`: The jump list was built and committed successfully
/// - `Err(e)`: A COM call failed; the caller logs the error and continues
unsafe fn try_update_jump_list(
    windows_data: &[Vec<DockMenuTab>],
    recent_files: &[PathBuf],
) -> windows::core::Result<()> {
    // COM may already be initialised by GPUI; S_FALSE here is not an error.
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let exe_path = std::env::current_exe().unwrap_or_default();
    let exe_hstr = HSTRING::from(exe_path.to_string_lossy().as_ref());
    let dest_list: ICustomDestinationList =
        CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
    // Associate this jump list with the app's taskbar button.
    let app_id = HSTRING::from("app.fulgur.fulgur");
    dest_list.SetAppID(PCWSTR(app_id.as_ptr()))?;
    let mut max_slots: u32 = 0;
    // BeginList returns items removed since the last commit; we discard them.
    let _removed: IObjectArray = dest_list.BeginList(&mut max_slots)?;

    // ── Tasks ────────────────────────────────────────────────────────────────
    // Both "New Tab" and "New Window" launch the executable without arguments,
    // which opens a new Fulgur instance with an empty tab — the closest Windows
    // equivalent to the macOS in-process actions.
    let tasks: IObjectCollection =
        CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
    tasks.AddObject(&make_shell_link(
        &exe_hstr,
        "--new-tab",
        "Open a new tab",
        "New Tab",
    )?)?;
    tasks.AddObject(&make_shell_link(
        &exe_hstr,
        "--new-window",
        "Open a new window",
        "New Window",
    )?)?;
    dest_list.AddUserTasks(&tasks.cast::<IObjectArray>()?)?;

    // ── Recent Files category ────────────────────────────────────────────────
    if !recent_files.is_empty() {
        let col: IObjectCollection =
            CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
        for file in recent_files {
            let name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Untitled");
            col.AddObject(&make_shell_link(
                &exe_hstr,
                file.to_string_lossy().as_ref(),
                name,
                name,
            )?)?;
        }
        let cat_name = HSTRING::from("Recent Files");
        dest_list.AppendCategory(PCWSTR(cat_name.as_ptr()), &col.cast::<IObjectArray>()?)?;
    }

    // ── Open Tabs category ───────────────────────────────────────────────────
    // Only file-backed tabs can be meaningfully re-opened from a jump list.
    let open_tabs: Vec<(&PathBuf, &str)> = windows_data
        .iter()
        .flat_map(|w| {
            w.iter().filter_map(|tab| match tab {
                DockMenuTab::File { path, name } => Some((path, name.as_ref())),
                DockMenuTab::Titled { .. } => None,
            })
        })
        .collect();

    if !open_tabs.is_empty() {
        let col: IObjectCollection =
            CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
        for (path, name) in &open_tabs {
            col.AddObject(&make_shell_link(
                &exe_hstr,
                path.to_string_lossy().as_ref(),
                name,
                name,
            )?)?;
        }
        let cat_name = HSTRING::from("Open Tabs");
        dest_list.AppendCategory(PCWSTR(cat_name.as_ptr()), &col.cast::<IObjectArray>()?)?;
    }

    dest_list.CommitList()?;
    Ok(())
}

/// Create an `IShellLinkW` that launches the given executable
///
/// Sets the path, optional arguments, tooltip description, and visible
/// jump-list label (via `IPropertyStore` / `PKEY_Title`).
///
/// ### Arguments
/// - `exe_hstr`: Wide-string path to the executable to launch
/// - `args`: Command-line arguments passed to the executable (empty string for none)
/// - `description`: Tooltip text shown when hovering over the item
/// - `title`: Visible label displayed in the jump list
///
/// ### Returns
/// - `Ok(IShellLinkW)`: The configured shell link ready to be added to the jump list
/// - `Err(e)`: A COM call failed
unsafe fn make_shell_link(
    exe_hstr: &HSTRING,
    args: &str,
    description: &str,
    title: &str,
) -> windows::core::Result<IShellLinkW> {
    let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
    link.SetPath(PCWSTR(exe_hstr.as_ptr()))?;
    if !args.is_empty() {
        let args_hstr = HSTRING::from(args);
        link.SetArguments(PCWSTR(args_hstr.as_ptr()))?;
    }
    if !description.is_empty() {
        let desc_hstr = HSTRING::from(description);
        link.SetDescription(PCWSTR(desc_hstr.as_ptr()))?;
    }
    let prop_store: IPropertyStore = link.cast()?;
    let mut pv = propvariant_from_str(title);
    let set_result = prop_store.SetValue(&PKEY_TITLE, &pv);
    let _ = prop_store.Commit();
    PropVariantClear(&mut pv).ok();
    set_result?;
    Ok(link)
}

/// Allocate a `VT_LPWSTR` `PROPVARIANT` from a Rust `&str`
///
/// Copies the string into `CoTaskMem`-owned wide-string memory so that
/// `PropVariantClear` can free it correctly. The caller is responsible for
/// calling `PropVariantClear` on the returned value when it is no longer needed.
///
/// ### Arguments
/// - `s`: The string to encode as a wide `PROPVARIANT`
///
/// ### Returns
/// - `PROPVARIANT`: A `VT_LPWSTR` variant owning a `CoTaskMem` allocation,
///   or a zeroed default if the allocation failed
unsafe fn propvariant_from_str(s: &str) -> PROPVARIANT {
    let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let byte_len = wide.len() * std::mem::size_of::<u16>();
    let ptr = CoTaskMemAlloc(byte_len) as *mut u16;
    let mut pv = PROPVARIANT::default();
    if !ptr.is_null() {
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        // Rust 2024: explicit `*` is required to deref through ManuallyDrop union
        // fields instead of relying on auto-deref (which is no longer applied).
        (*pv.Anonymous.Anonymous).vt = VARENUM(VT_LPWSTR.0);
        (*pv.Anonymous.Anonymous).Anonymous.pwszVal = PWSTR(ptr);
    }
    pv
}

#[cfg(test)]
mod tests {
    use super::update_windows_jump_list;
    use std::path::PathBuf;

    /// Smoke-test: calling with empty data must not panic.
    #[test]
    fn test_update_windows_jump_list_empty_data_does_not_panic() {
        // Will log a warning in headless/CI environments but must never panic.
        update_windows_jump_list(&[], &[]);
    }

    /// Smoke-test: calling with realistic data must not panic.
    #[test]
    fn test_update_windows_jump_list_with_data_does_not_panic() {
        use crate::fulgur::ui::menus::DockMenuTab;
        let tabs = vec![vec![DockMenuTab::File {
            name: gpui::SharedString::from("main.rs"),
            path: PathBuf::from(r"C:\Users\user\project\main.rs"),
        }]];
        let recent = vec![PathBuf::from(r"C:\Users\user\project\main.rs")];
        update_windows_jump_list(&tabs, &recent);
    }
}
