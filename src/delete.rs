//! Delete a file through the Windows Recycle Bin.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

const FO_DELETE: u32 = 3;
const FOF_SILENT: u16 = 0x0004;
const FOF_NOCONFIRMATION: u16 = 0x0010;
const FOF_ALLOWUNDO: u16 = 0x0040;
const FOF_NOERRORUI: u16 = 0x0400;

#[repr(C)]
struct FileOp {
    hwnd: *mut c_void,
    action: u32,
    from: *const u16,
    to: *const u16,
    flags: u16,
    aborted: i32,
    name_mappings: *mut c_void,
    progress_title: *const u16,
}

pub fn is_network(path: &Path) -> bool {
    let text = path.to_string_lossy();
    if text.starts_with(r"\\") && !text.starts_with(r"\\?\") {
        return true;
    }
    if text.starts_with(r"\\?\UNC\") {
        return true;
    }
    if let Some(drive) = text.chars().next() {
        if drive.is_ascii_alphabetic() && text.as_bytes().get(1) == Some(&b':') {
            return drive_type(drive) == 4; // DRIVE_REMOTE
        }
    }
    false
}

fn drive_type(drive: char) -> u32 {
    let root = format!("{drive}:\\");
    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { GetDriveTypeW(wide.as_ptr()) }
}

pub fn to_recycle_bin(path: &Path) -> Result<(), String> {
    let network = is_network(path);
    let path = path
        .canonicalize()
        .map_err(|error| format!("delete path: {error}"))?;
    if !path.is_file() {
        return Err("file no longer exists".to_owned());
    }

    // Network shares do not have a Recycle Bin; Explorer deletes there
    // permanently, so do the same after the confirmation dialog.
    if network {
        return std::fs::remove_file(&path)
            .map_err(|error| format!("delete network file: {error}"));
    }

    let mut text = path.display().to_string();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        text = format!(r"\\{rest}");
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        text = rest.to_owned();
    }
    let mut from: Vec<u16> = std::ffi::OsStr::new(&text)
        .encode_wide()
        .chain([0, 0])
        .collect();
    let to = [0u16, 0];
    let mut operation = FileOp {
        hwnd: std::ptr::null_mut(),
        action: FO_DELETE,
        from: from.as_mut_ptr(),
        to: to.as_ptr(),
        flags: FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI,
        aborted: 0,
        name_mappings: std::ptr::null_mut(),
        progress_title: std::ptr::null(),
    };

    let code = unsafe { SHFileOperationW(&mut operation) };
    if code == 0 && operation.aborted == 0 {
        Ok(())
    } else if operation.aborted != 0 {
        Err("delete was cancelled".to_owned())
    } else {
        // Some filesystems/long paths do not support the shell recycle
        // operation (ERROR_CALL_NOT_IMPLEMENTED = 120). Fall back to a
        // normal delete rather than leaving the user stuck.
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) => Err(format!(
                "Windows could not delete the file ({code}); {error}"
            )),
        }
    }
}

#[link(name = "shell32")]
extern "system" {
    fn SHFileOperationW(operation: *mut FileOp) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetDriveTypeW(root: *const u16) -> u32;
}
