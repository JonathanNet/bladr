//! Register Bladr as a photo viewer and open Windows' default-apps UI.
//!
//! Windows 10/11 does not allow an application to silently become the default
//! handler. We register a ProgID and Capabilities, then let the user confirm
//! the choice in Windows Settings.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::process::Command;

const HKEY_CURRENT_USER: usize = 0x8000_0001;
const KEY_WRITE: u32 = 0x0002_0006;
const REG_SZ: u32 = 1;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SW_SHOWNORMAL: i32 = 1;

const PROG_ID: &str = "Bladr.Image";
const ASSOCSTR_EXECUTABLE: u32 = 2;
const EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "tif", "tiff", "webp", "heic", "heif",
];

pub fn register() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| format!("current exe: {error}"))?;
    let exe = exe.display().to_string();
    let command = format!("\"{exe}\" \"%1\"");

    set_value(
        &format!(r"Software\Classes\{PROG_ID}"),
        None,
        "Bladr billede",
    )?;
    set_value(
        &format!(r"Software\Classes\{PROG_ID}\DefaultIcon"),
        None,
        &format!("\"{exe}\",0"),
    )?;
    set_value(
        &format!(r"Software\Classes\{PROG_ID}\shell\open\command"),
        None,
        &command,
    )?;

    set_value(
        r"Software\Classes\Applications\Bladr.exe\shell\open\command",
        None,
        &command,
    )?;
    set_value(
        r"Software\Classes\Applications\Bladr.exe",
        Some("FriendlyAppName"),
        "Bladr",
    )?;

    set_value(
        r"Software\Bladr\Capabilities",
        Some("ApplicationName"),
        "Bladr",
    )?;
    set_value(
        r"Software\Bladr\Capabilities",
        Some("ApplicationDescription"),
        "Photo viewer",
    )?;
    for extension in EXTENSIONS {
        set_value(
            r"Software\Bladr\Capabilities\FileAssociations",
            Some(&format!(".{extension}")),
            PROG_ID,
        )?;
        set_value(
            &format!(r"Software\Classes\.{extension}\OpenWithProgids"),
            Some(PROG_ID),
            "",
        )?;
    }

    set_value(
        r"Software\RegisteredApplications",
        Some("Bladr"),
        r"Software\Bladr\Capabilities",
    )?;
    Ok(())
}

/// True when Windows currently opens any common image type with this Bladr.
pub fn is_default() -> bool {
    for extension in EXTENSIONS {
        if handler_matches(extension) {
            return true;
        }
    }
    for extension in ["jpg", "jpeg", "png"] {
        if is_default_for(&format!(".{extension}")) {
            return true;
        }
    }
    false
}

fn handler_matches(extension: &str) -> bool {
    let extension = wide(&format!(".{extension}"));
    let operation = wide("open");
    let mut buffer = vec![0u16; 2048];
    let mut length = buffer.len() as u32;
    let result = unsafe {
        AssocQueryStringW(
            0,
            ASSOCSTR_EXECUTABLE,
            extension.as_ptr(),
            operation.as_ptr(),
            buffer.as_mut_ptr(),
            &mut length,
        )
    };
    if result != 0 {
        return false;
    }
    let length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    let handler = String::from_utf16_lossy(&buffer[..length]);
    let Ok(current) = std::env::current_exe() else {
        return false;
    };
    let current = current.canonicalize().unwrap_or(current);
    let handler = std::path::PathBuf::from(handler);
    let handler = handler.canonicalize().unwrap_or(handler);
    handler
        .to_string_lossy()
        .eq_ignore_ascii_case(&current.to_string_lossy())
}

fn is_default_for(extension: &str) -> bool {
    let key = format!(
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{extension}\UserChoice"
    );
    let output = Command::new("reg")
        .args(["query", &key, "/v", "ProgId"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let Ok(output) = output else {
        return false;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find(|line| line.contains("ProgId"))
        .and_then(|line| line.split_once("REG_SZ"))
        .map(|(_, value)| {
            let value = value.trim();
            value.eq_ignore_ascii_case(PROG_ID) || value.to_ascii_lowercase().contains("bladr")
        })
        .unwrap_or(false)
}

/// Opens the Windows page where the user can confirm Bladr as the default.
pub fn open_default_apps() -> Result<(), String> {
    let uri = wide("ms-settings:defaultapps?registeredAppUser=Bladr");
    let verb = wide("open");
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            uri.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize > 32 {
        return Ok(());
    }

    let fallback = wide("ms-settings:defaultapps");
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            fallback.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize > 32 {
        Ok(())
    } else {
        Err("Windows Settings could not be opened".to_owned())
    }
}

fn set_value(key_path: &str, name: Option<&str>, value: &str) -> Result<(), String> {
    let key = create_key(key_path)?;
    let name_wide = name.map(|name| wide(name));
    let name_ptr = name_wide
        .as_ref()
        .map(|value| value.as_ptr())
        .unwrap_or(std::ptr::null());
    let value = wide(value);
    let bytes = unsafe {
        std::slice::from_raw_parts(value.as_ptr() as *const u8, value.len() * 2)
    };
    let result = unsafe {
        RegSetValueExW(
            key,
            name_ptr,
            0,
            REG_SZ,
            bytes.as_ptr(),
            bytes.len() as u32,
        )
    };
    unsafe {
        RegCloseKey(key);
    }
    if result == 0 {
        Ok(())
    } else {
        Err(format!("registry write failed ({result})"))
    }
}

fn create_key(path: &str) -> Result<usize, String> {
    let mut current = HKEY_CURRENT_USER;
    for part in path.split('\\') {
        let subkey = wide(part);
        let mut next = 0usize;
        let mut disposition = 0u32;
        let result = unsafe {
            RegCreateKeyExW(
                current,
                subkey.as_ptr(),
                0,
                std::ptr::null(),
                0,
                KEY_WRITE,
                std::ptr::null_mut(),
                &mut next,
                &mut disposition,
            )
        };
        if result != 0 {
            if current != HKEY_CURRENT_USER {
                unsafe {
                    RegCloseKey(current);
                }
            }
            return Err(format!("registry key {path} failed ({result})"));
        }
        if current != HKEY_CURRENT_USER {
            unsafe {
                RegCloseKey(current);
            }
        }
        current = next;
    }
    Ok(current)
}

fn wide(text: &str) -> Vec<u16> {
    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[link(name = "advapi32")]
extern "system" {
    fn RegCreateKeyExW(
        key: usize,
        sub_key: *const u16,
        reserved: u32,
        class: *const u16,
        options: u32,
        sam: u32,
        security: *mut c_void,
        result: *mut usize,
        disposition: *mut u32,
    ) -> i32;
    fn RegSetValueExW(
        key: usize,
        value_name: *const u16,
        reserved: u32,
        value_type: u32,
        data: *const u8,
        data_size: u32,
    ) -> i32;
    fn RegCloseKey(key: usize) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    fn ShellExecuteW(
        window: *mut c_void,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> isize;
}

#[link(name = "shlwapi")]
extern "system" {
    fn AssocQueryStringW(
        flags: u32,
        string: u32,
        association: *const u16,
        extra: *const u16,
        output: *mut u16,
        length: *mut u32,
    ) -> i32;
}
