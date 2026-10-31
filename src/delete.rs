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

pub fn to_recycle_bin(path: &Path) -> Result<(), String> {
    let path = path
        .canonicalize()
        .map_err(|error| format!("delete path: {error}"))?;
    if !path.is_file() {
        return Err("file no longer exists".to_owned());
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
        Err(format!("Windows could not delete the file ({code})"))
    }
}

#[link(name = "shell32")]
extern "system" {
    fn SHFileOperationW(operation: *mut FileOp) -> i32;
}
