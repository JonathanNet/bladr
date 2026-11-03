//! Open Explorer with one file selected.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

const COINIT_APARTMENTTHREADED: u32 = 0x0000_0002;

pub fn reveal(path: &Path) -> Result<(), String> {
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let com = unsafe { CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED) };
    let pidl = unsafe { ILCreateFromPathW(wide.as_ptr()) };
    if !pidl.is_null() {
        let result = unsafe { SHOpenFolderAndSelectItems(pidl, 0, std::ptr::null(), 0) };
        unsafe {
            ILFree(pidl);
        }
        if com == 0 || com == 1 {
            unsafe { CoUninitialize() };
        }
        if result >= 0 {
            return Ok(());
        }
    } else if com == 0 || com == 1 {
        unsafe { CoUninitialize() };
    }

    // Some elevated/restricted shells reject the PIDL API. Explorer's
    // /select switch is a useful fallback, but it needs the raw command line
    // so the quotes are not escaped twice.
    let argument = format!("/select,\"{}\"", path.display());
    Command::new("explorer.exe")
        .raw_arg(argument)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("explorer.exe: {error}"))
}

#[link(name = "shell32")]
extern "system" {
    fn ILCreateFromPathW(path: *const u16) -> *mut c_void;
    fn SHOpenFolderAndSelectItems(
        folder: *const c_void,
        count: u32,
        items: *const *const c_void,
        flags: u32,
    ) -> i32;
    fn ILFree(pidl: *mut c_void);
}

#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, coinit: u32) -> i32;
    fn CoUninitialize();
}
