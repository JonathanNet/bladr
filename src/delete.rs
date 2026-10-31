//! Delete a file through the Windows Recycle Bin, with an elevated fallback.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

const FO_DELETE: u32 = 3;
const FOF_SILENT: u16 = 0x0004;
const FOF_NOCONFIRMATION: u16 = 0x0010;
const FOF_ALLOWUNDO: u16 = 0x0040;
const FOF_NOERRORUI: u16 = 0x0400;
const FILE_ATTRIBUTE_READONLY: u32 = 0x0000_0001;
const COINIT_APARTMENTTHREADED: u32 = 0x0000_0002;
const SEE_MASK_NOCLOSEPROCESS: u32 = 0x0000_0040;
const INFINITE: u32 = 0xFFFF_FFFF;

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

#[repr(C)]
struct ShellExecuteInfo {
    size: u32,
    mask: u32,
    hwnd: *mut c_void,
    verb: *const u16,
    file: *const u16,
    parameters: *const u16,
    directory: *const u16,
    show: i32,
    inst_app: *mut c_void,
    id_list: *mut c_void,
    class: *const u16,
    hkey_class: *mut c_void,
    hot_key: u32,
    icon: *mut c_void,
    process: *mut c_void,
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

fn clear_readonly(path: &Path) -> Result<(), String> {
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let attributes = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if attributes == u32::MAX {
        return Err(format!(
            "read attributes (Windows error {})",
            unsafe { GetLastError() }
        ));
    }
    if attributes & FILE_ATTRIBUTE_READONLY == 0 {
        return Ok(());
    }
    let result = unsafe {
        SetFileAttributesW(wide.as_ptr(), attributes & !FILE_ATTRIBUTE_READONLY)
    };
    if result == 0 {
        Err(format!(
            "clear read-only attribute (Windows error {})",
            unsafe { GetLastError() }
        ))
    } else {
        Ok(())
    }
}

pub fn to_recycle_bin(path: &Path) -> Result<(), String> {
    let path = path
        .canonicalize()
        .map_err(|error| format!("delete path: {error}"))?;
    if !path.is_file() {
        return Err("file no longer exists".to_owned());
    }

    // Windows refuses to delete read-only files, even when Explorer would
    // offer to remove the attribute. Do it after the user has confirmed.
    clear_readonly(&path)?;

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

    // SHFileOperation is a shell API and must run on a COM-initialized
    // thread. Without this it can return ERROR_CALL_NOT_IMPLEMENTED (120)
    // on worker threads.
    let com = unsafe { CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED) };
    let code = unsafe { SHFileOperationW(&mut operation) };
    if com == 0 || com == 1 {
        unsafe {
            CoUninitialize();
        }
    }

    if code == 0 && operation.aborted == 0 {
        Ok(())
    } else if operation.aborted != 0 {
        Err("delete was cancelled".to_owned())
    } else {
        // Some filesystems/long paths do not support the shell recycle
        // operation. Fall back to a normal delete rather than leaving the
        // user stuck.
        match remove_with_retry(&path) {
            Ok(()) => Ok(()),
            Err(error) => Err(format!(
                "Windows could not delete the file ({code}); {error}"
            )),
        }
    }
}

/// A decoder or thumbnail provider may keep the file open for a moment after
/// the user confirms. Retry deletion briefly before giving up.
fn remove_with_retry(path: &Path) -> std::io::Result<()> {
    let mut last = None;
    for attempt in 0..8 {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) => {
                let retry = matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::WouldBlock
                );
                last = Some(error);
                if !retry {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(120 + attempt * 40));
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("delete failed")))
}

pub fn delete_elevated(path: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| format!("current exe: {error}"))?;
    let parameters = format!("--delete \"{}\"", path.display());
    let verb = wide(std::ffi::OsStr::new("runas"));
    let file = wide(exe.as_os_str());
    let parameters = wide(std::ffi::OsStr::new(&parameters));

    let mut info = ShellExecuteInfo {
        size: std::mem::size_of::<ShellExecuteInfo>() as u32,
        mask: SEE_MASK_NOCLOSEPROCESS,
        hwnd: std::ptr::null_mut(),
        verb: verb.as_ptr(),
        file: file.as_ptr(),
        parameters: parameters.as_ptr(),
        directory: std::ptr::null(),
        show: 0,
        inst_app: std::ptr::null_mut(),
        id_list: std::ptr::null_mut(),
        class: std::ptr::null(),
        hkey_class: std::ptr::null_mut(),
        hot_key: 0,
        icon: std::ptr::null_mut(),
        process: std::ptr::null_mut(),
    };

    let com = unsafe { CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED) };
    let launched = unsafe { ShellExecuteExW(&mut info) };
    if com == 0 || com == 1 {
        unsafe {
            CoUninitialize();
        }
    }
    if launched == 0 {
        return Err(format!(
            "administrator prompt failed (Windows error {})",
            unsafe { GetLastError() }
        ));
    }
    if info.process.is_null() {
        return Err("administrator process did not start".to_owned());
    }

    unsafe {
        WaitForSingleObject(info.process, INFINITE);
    }
    let mut exit_code = 1u32;
    unsafe {
        GetExitCodeProcess(info.process, &mut exit_code);
        CloseHandle(info.process);
    }
    if exit_code == 0 {
        Ok(())
    } else {
        Err("administrator could not delete the file".to_owned())
    }
}

fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

#[link(name = "shell32")]
extern "system" {
    fn SHFileOperationW(operation: *mut FileOp) -> i32;
    fn ShellExecuteExW(info: *mut ShellExecuteInfo) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetDriveTypeW(root: *const u16) -> u32;
    fn GetFileAttributesW(path: *const u16) -> u32;
    fn SetFileAttributesW(path: *const u16, attributes: u32) -> i32;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn GetExitCodeProcess(process: *mut c_void, exit_code: *mut u32) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn GetLastError() -> u32;
}

#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, coinit: u32) -> i32;
    fn CoUninitialize();
}
