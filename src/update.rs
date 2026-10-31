//! GitHub release check and installer download over WinHTTP.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const REPO: &str = "JonathanNet/bladr";
const LATEST_PAGE: &str = "https://github.com/JonathanNet/bladr/releases/latest";
const API_LATEST: &str = "https://api.github.com/repos/JonathanNet/bladr/releases/latest";
const MAX_JSON: usize = 512 * 1024;
const MAX_INSTALLER: usize = 64 * 1024 * 1024;
const MAX_PAGE: usize = 8 * 1024 * 1024;

struct Response {
    body: Vec<u8>,
    url: String,
}

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
}

pub fn check(current: &str) -> Result<Option<UpdateInfo>, String> {
    let version = latest_version()?;
    if !is_newer(&version, current) {
        return Ok(None);
    }

    let url = format!(
        "https://github.com/{REPO}/releases/download/v{version}/Bladr-{version}-Setup.exe"
    );
    Ok(Some(UpdateInfo { version, url }))
}

pub fn latest_version() -> Result<String, String> {
    // `/releases/latest` redirects to `/releases/tag/vX.Y.Z`; this avoids the
    // GitHub API's 60-requests-per-hour limit for unauthenticated clients.
    if let Ok(page) = http_get(LATEST_PAGE, MAX_PAGE) {
        if let Some(tag) = page.url.split("/tag/").nth(1) {
            let version = tag
                .split(['/', '?', '#'])
                .next()
                .unwrap_or(tag)
                .trim_start_matches('v');
            if !version.is_empty() {
                return Ok(version.to_owned());
            }
        }
    }

    let body = http_get(API_LATEST, MAX_JSON)?.body;
    let body = String::from_utf8_lossy(&body);
    let tag = json_string(&body, "tag_name").ok_or("release has no tag_name")?;
    Ok(tag.trim_start_matches('v').to_owned())
}

pub fn download_path(version: &str) -> Result<PathBuf, String> {
    let root = std::env::var_os("BLADR_UPDATE_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .ok_or_else(|| "LOCALAPPDATA is not set".to_owned())?;
    let dir = root.join("Bladr").join("Updates");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(format!("Bladr-{version}-Setup.exe")))
}

pub fn download(info: &UpdateInfo) -> Result<PathBuf, String> {
    let path = download_path(&info.version)?;
    let bytes = http_get(&info.url, MAX_INSTALLER)?.body;
    let temp = path.with_extension("download");
    std::fs::write(&temp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn run_installer(path: &Path) -> Result<(), String> {
    Command::new(path)
        .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART"])
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn is_newer(candidate: &str, current: &str) -> bool {
    let a = parse_version(candidate);
    let b = parse_version(current);
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

fn parse_version(text: &str) -> Vec<u32> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

fn json_string(body: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\":\"");
    let start = body.find(&pattern)? + pattern.len();
    let rest = &body[start..];
    let end = rest.find('"')?;
    Some(rest[..end].replace("\\/", "/"))
}

fn http_get(url: &str, limit: usize) -> Result<Response, String> {
    let (host, path) = split_url(url).ok_or("only https URLs are supported")?;
    let agent = wide(&format!("Bladr/{}", env!("CARGO_PKG_VERSION")));
    let host = wide(host);
    let path = wide(&path);

    unsafe {
        let session = WinHttpOpen(agent.as_ptr(), 0, std::ptr::null(), std::ptr::null(), 0);
        if session.is_null() {
            return Err("WinHttpOpen failed".to_owned());
        }
        WinHttpSetTimeouts(session, 5000, 5000, 10000, 15000);

        let connect = WinHttpConnect(session, host.as_ptr(), 443, 0);
        if connect.is_null() {
            WinHttpCloseHandle(session);
            return Err("WinHttpConnect failed".to_owned());
        }

        let request = WinHttpOpenRequest(
            connect,
            wide("GET").as_ptr(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0x0080_0000, // WINHTTP_FLAG_SECURE
        );
        if request.is_null() {
            WinHttpCloseHandle(connect);
            WinHttpCloseHandle(session);
            return Err("WinHttpOpenRequest failed".to_owned());
        }

        let mut policy: u32 = 2; // WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS
        WinHttpSetOption(
            request,
            88,
            &mut policy as *mut _ as *mut c_void,
            std::mem::size_of::<u32>() as u32,
        );

        let sent = WinHttpSendRequest(
            request,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            0,
            0,
        );
        if sent == 0 {
            let code = last_error();
            WinHttpCloseHandle(request);
            WinHttpCloseHandle(connect);
            WinHttpCloseHandle(session);
            return Err(format!("WinHttpSendRequest failed ({code})"));
        }
        if WinHttpReceiveResponse(request, std::ptr::null_mut()) == 0 {
            let code = last_error();
            WinHttpCloseHandle(request);
            WinHttpCloseHandle(connect);
            WinHttpCloseHandle(session);
            return Err(format!("WinHttpReceiveResponse failed ({code})"));
        }

        let mut status: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        let queried = WinHttpQueryHeaders(
            request,
            0x2000_0013, // STATUS_CODE | FLAG_NUMBER
            std::ptr::null(),
            &mut status as *mut _ as *mut c_void,
            &mut len,
            std::ptr::null_mut(),
        );
        if queried == 0 || !(200..300).contains(&status) {
            WinHttpCloseHandle(request);
            WinHttpCloseHandle(connect);
            WinHttpCloseHandle(session);
            return Err(format!("HTTP status {status}"));
        }

        let mut out = Vec::new();
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let mut read = 0u32;
            if WinHttpReadData(
                request,
                buffer.as_mut_ptr() as *mut c_void,
                buffer.len() as u32,
                &mut read,
            ) == 0
            {
                WinHttpCloseHandle(request);
                WinHttpCloseHandle(connect);
                WinHttpCloseHandle(session);
                return Err("WinHttpReadData failed".to_owned());
            }
            if read == 0 {
                break;
            }
            out.extend_from_slice(&buffer[..read as usize]);
            if out.len() > limit {
                WinHttpCloseHandle(request);
                WinHttpCloseHandle(connect);
                WinHttpCloseHandle(session);
                return Err("response too large".to_owned());
            }
        }

        let mut final_url = url.to_owned();
        let mut url_buffer = vec![0u16; 2048];
        let mut url_len = (url_buffer.len() * 2) as u32;
        if WinHttpQueryOption(
            request,
            34, // WINHTTP_OPTION_URL
            url_buffer.as_mut_ptr() as *mut c_void,
            &mut url_len,
        ) != 0
        {
            let chars = url_buffer
                .iter()
                .position(|&ch| ch == 0)
                .unwrap_or(url_buffer.len());
            final_url = String::from_utf16_lossy(&url_buffer[..chars]);
        }

        WinHttpCloseHandle(request);
        WinHttpCloseHandle(connect);
        WinHttpCloseHandle(session);
        Ok(Response {
            body: out,
            url: final_url,
        })
    }
}

fn split_url(url: &str) -> Option<(&str, String)> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    if host.is_empty() {
        return None;
    }
    Some((host, format!("/{path}")))
}

fn wide(text: &str) -> Vec<u16> {
    std::ffi::OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn last_error() -> u32 {
    unsafe { GetLastError() }
}

#[link(name = "kernel32")]
extern "system" {
    fn GetLastError() -> u32;
}

#[link(name = "winhttp")]
extern "system" {
    fn WinHttpOpen(
        agent: *const u16,
        access_type: u32,
        proxy: *const u16,
        bypass: *const u16,
        flags: u32,
    ) -> *mut c_void;
    fn WinHttpConnect(
        session: *mut c_void,
        server: *const u16,
        port: u16,
        reserved: u32,
    ) -> *mut c_void;
    fn WinHttpOpenRequest(
        connect: *mut c_void,
        verb: *const u16,
        object: *const u16,
        version: *const u16,
        referrer: *const u16,
        accept_types: *const *const u16,
        flags: u32,
    ) -> *mut c_void;
    fn WinHttpSendRequest(
        request: *mut c_void,
        headers: *const u16,
        headers_length: u32,
        optional: *mut c_void,
        optional_length: u32,
        total_length: u32,
        context: usize,
    ) -> i32;
    fn WinHttpReceiveResponse(request: *mut c_void, reserved: *mut c_void) -> i32;
    fn WinHttpQueryHeaders(
        request: *mut c_void,
        info_level: u32,
        name: *const u16,
        buffer: *mut c_void,
        buffer_length: *mut u32,
        index: *mut u32,
    ) -> i32;
    fn WinHttpReadData(
        request: *mut c_void,
        buffer: *mut c_void,
        to_read: u32,
        read: *mut u32,
    ) -> i32;
    fn WinHttpSetTimeouts(
        session: *mut c_void,
        resolve: i32,
        connect: i32,
        send: i32,
        receive: i32,
    ) -> i32;
    fn WinHttpSetOption(
        handle: *mut c_void,
        option: u32,
        buffer: *mut c_void,
        length: u32,
    ) -> i32;
    fn WinHttpQueryOption(
        handle: *mut c_void,
        option: u32,
        buffer: *mut c_void,
        length: *mut u32,
    ) -> i32;
    fn WinHttpCloseHandle(handle: *mut c_void) -> i32;
}
