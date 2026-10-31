//! Tiny launcher that gives Inno Setup a writable temp directory.
//!
//! Inno's installer extracts itself into `%TEMP%` before its own code runs.
//! On machines where `%TEMP%` is locked down (or when the file is started by a
//! restricted process), Setup fails with "unable to create the directory
//! ...is-*.tmp". This launcher writes the real setup to `%LOCALAPPDATA%` and
//! runs it with TEMP/TMP pointed at a folder it owns.

#![windows_subsystem = "windows"]

use std::env;
use std::ffi::c_void;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const INNER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../Output/Bladr-",
    env!("CARGO_PKG_VERSION"),
    "-Setup-inner.exe"
));

fn main() {
    if let Err(error) = run() {
        message(&error);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let root = setup_root()?;
    let temp = root.join("Temp");
    let cache = root.join("Cache");
    fs::create_dir_all(&temp)
        .map_err(|e| format!("Kunne ikke oprette midlertidig mappe:\n{temp:?}\n{e}"))?;
    fs::create_dir_all(&cache)
        .map_err(|e| format!("Kunne ikke oprette cache-mappe:\n{cache:?}\n{e}"))?;

    let inner = cache.join(format!("Bladr-Setup-{}.exe", std::process::id()));
    fs::write(&inner, INNER)
        .map_err(|e| format!("Kunne ikke udpakke installationsprogrammet:\n{inner:?}\n{e}"))?;

    let status = Command::new(&inner)
        .args(env::args_os().skip(1))
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .status()
        .map_err(|e| format!("Kunne ikke starte installationsprogrammet:\n{e}"))?;

    let _ = fs::remove_file(&inner);
    let _ = fs::remove_dir_all(&temp);

    if !status.success() {
        return Err(format!(
            "Installationsprogrammet sluttede med kode {}.",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

fn setup_root() -> Result<PathBuf, String> {
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(root) = env::var_os("BLADR_SETUP_ROOT") {
        bases.push(PathBuf::from(root));
    }
    if let Some(local) = env::var_os("LOCALAPPDATA") {
        bases.push(PathBuf::from(local));
    }
    if let Some(profile) = env::var_os("USERPROFILE") {
        bases.push(PathBuf::from(profile).join("AppData").join("Local"));
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            bases.push(parent.to_path_buf());
        }
    }

    for base in bases {
        let root = base.join("Bladr").join("Setup");
        if fs::create_dir_all(&root).is_ok() {
            return Ok(root);
        }
    }

    Err("Kunne ikke finde en skrivbar mappe til installationen.".to_owned())
}

fn message(text: &str) {
    let text = wide(text);
    let caption = wide("Bladr Setup");
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10);
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(
        hwnd: *mut c_void,
        text: *const u16,
        caption: *const u16,
        flags: u32,
    ) -> i32;
}
