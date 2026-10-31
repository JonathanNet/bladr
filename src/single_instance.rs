//! Keep a single Bladr process: a second launch hands its image to the first.
//!
//! A named mutex elects the primary instance. The primary listens on a named
//! pipe; later instances send the requested path (or an empty message to just
//! focus the existing window) and exit.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

const ERROR_ALREADY_EXISTS: u32 = 183;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_PIPE_BUSY: u32 = 231;
const ERROR_PIPE_CONNECTED: u32 = 535;

const PIPE_ACCESS_INBOUND: u32 = 0x0000_0001;
const PIPE_TYPE_BYTE: u32 = 0x0000_0000;
const PIPE_READMODE_BYTE: u32 = 0x0000_0000;
const PIPE_WAIT: u32 = 0x0000_0000;
const PIPE_UNLIMITED_INSTANCES: u32 = 255;

const GENERIC_WRITE: u32 = 0x0010_0002; // FILE_WRITE_DATA | SYNCHRONIZE
const OPEN_EXISTING: u32 = 3;

fn suffix() -> String {    let user = std::env::var("USERNAME").unwrap_or_else(|_| "default".to_owned());
    let cleaned: String = user
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect();
    if cleaned.is_empty() {
        "default".to_owned()
    } else {
        cleaned
    }
}

fn pipe_name() -> Vec<u16> {
    wide(&format!(r"\\.\pipe\BladrSingleInstance-{}", suffix()))
}

fn mutex_name() -> Vec<u16> {
    wide(&format!(r"Local\BladrSingleInstanceMutex-{}", suffix()))
}

/// Returns a receiver for the primary instance, or `None` if Bladr is already
/// running. In the latter case the caller should call [`notify_existing`].
pub fn acquire() -> Option<Receiver<PathBuf>> {
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name().as_ptr()) };
    if mutex.is_null() {
        // If the mutex cannot be created, run anyway rather than losing the app.
        return Some(start_server());
    }

    let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if already_running {
        unsafe {
            CloseHandle(mutex);
        }
        return None;
    }

    // Keep the mutex handle alive for the whole process; the OS closes it.
    Some(start_server())
}

fn start_server() -> Receiver<PathBuf> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || server(tx));
    rx
}

fn server(tx: mpsc::Sender<PathBuf>) {
    let name = pipe_name();
    let sddl = wide("D:(A;;GA;;;WD)");
    let mut descriptor: *mut c_void = std::ptr::null_mut();
    let mut descriptor_size = 0u32;
    let has_descriptor = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            &mut descriptor_size,
        )
    } != 0;
    let mut attributes = SecurityAttributes {
        length: std::mem::size_of::<SecurityAttributes>() as u32,
        descriptor,
        inherit: 0,
    };
    let attributes_ptr = if has_descriptor {
        &mut attributes as *mut _ as *mut c_void
    } else {
        std::ptr::null_mut()
    };

    loop {
        let pipe = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_INBOUND,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                4096,
                4096,
                0,
                attributes_ptr,
            )
        };
        if pipe == INVALID_HANDLE_VALUE {
            thread::sleep(Duration::from_millis(200));
            continue;
        }

        let connected = unsafe { ConnectNamedPipe(pipe, std::ptr::null_mut()) } != 0
            || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if connected {
            if let Some(path) = read_message(pipe) {
                let _ = tx.send(path);
            }
        }

        unsafe {
            DisconnectNamedPipe(pipe);
            CloseHandle(pipe);
        }
    }
}

fn read_message(pipe: *mut c_void) -> Option<PathBuf> {
    let mut header = [0u8; 4];
    if !read_exact(pipe, &mut header) {
        return None;
    }
    let len = u32::from_le_bytes(header) as usize;
    if len > 64 * 1024 {
        return None;
    }
    let mut bytes = vec![0u8; len];
    if len > 0 && !read_exact(pipe, &mut bytes) {
        return None;
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Some(PathBuf::from(text))
}

fn read_exact(pipe: *mut c_void, buffer: &mut [u8]) -> bool {
    let mut offset = 0usize;
    while offset < buffer.len() {
        let mut read = 0u32;
        let ok = unsafe {
            ReadFile(
                pipe,
                buffer[offset..].as_mut_ptr() as *mut c_void,
                (buffer.len() - offset) as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || read == 0 {
            return false;
        }
        offset += read as usize;
    }
    true
}

/// Send a path (or an empty path to activate the window) to the existing app.
pub fn notify_existing(path: Option<&Path>) {
    let bytes = path
        .map(|p| p.to_string_lossy().into_owned().into_bytes())
        .unwrap_or_default();
    let mut message = Vec::with_capacity(bytes.len() + 4);
    message.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    message.extend_from_slice(&bytes);

    let name = pipe_name();
    for _ in 0..80 {
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_WRITE,
                0,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            let mut written = 0u32;
            unsafe {
                WriteFile(
                    handle,
                    message.as_ptr() as *const c_void,
                    message.len() as u32,
                    &mut written,
                    std::ptr::null_mut(),
                );
                CloseHandle(handle);
            }
            return;
        }

        let error = unsafe { GetLastError() };
        if error == ERROR_PIPE_BUSY {
            unsafe {
                WaitNamedPipeW(name.as_ptr(), 500);
            }
        } else if error == ERROR_FILE_NOT_FOUND {
            thread::sleep(Duration::from_millis(100));
        } else {
            thread::sleep(Duration::from_millis(100));
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

const INVALID_HANDLE_VALUE: *mut c_void = -1isize as *mut c_void;

#[repr(C)]
struct SecurityAttributes {
    length: u32,
    descriptor: *mut c_void,
    inherit: i32,
}

#[link(name = "advapi32")]
extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        sddl: *const u16,
        revision: u32,
        descriptor: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(
        attributes: *const c_void,
        initial_owner: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn CreateNamedPipeW(
        name: *const u16,
        open_mode: u32,
        pipe_mode: u32,
        max_instances: u32,
        out_buffer_size: u32,
        in_buffer_size: u32,
        default_timeout: u32,
        security_attributes: *mut c_void,
    ) -> *mut c_void;
    fn ConnectNamedPipe(pipe: *mut c_void, overlapped: *mut c_void) -> i32;
    fn DisconnectNamedPipe(pipe: *mut c_void) -> i32;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut c_void,
        creation: u32,
        flags: u32,
        template: *mut c_void,
    ) -> *mut c_void;
    fn ReadFile(
        file: *mut c_void,
        buffer: *mut c_void,
        to_read: u32,
        read: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn WriteFile(
        file: *mut c_void,
        buffer: *const c_void,
        to_write: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn WaitNamedPipeW(name: *const u16, timeout: u32) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn GetLastError() -> u32;
}
