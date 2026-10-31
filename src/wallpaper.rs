//! Set the current image as the Windows desktop wallpaper.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::i18n::Lang;

const SPI_SETDESKWALLPAPER: u32 = 0x0014;
const SPIF_UPDATEINIFILE: u32 = 0x0001;
const SPIF_SENDCHANGE: u32 = 0x0002;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy)]
pub enum Style {
    Centered,
    Tiled,
    Stretched,
}

impl Style {
    fn values(self) -> (&'static str, &'static str) {
        match self {
            Self::Centered => ("0", "0"),
            Self::Tiled => ("0", "1"),
            Self::Stretched => ("2", "0"),
        }
    }
}

pub fn set(image: &Path, style: Style, lang: Lang) -> Result<(), String> {
    let wallpaper = wallpaper_image(image, lang)?;
    backup()?;
    let (wallpaper_style, tile) = style.values();
    set_registry("WallpaperStyle", wallpaper_style)?;
    set_registry("TileWallpaper", tile)?;
    apply(&wallpaper)
}

pub fn restore() -> Result<(), String> {
    let Some((path, style, tile)) = read_backup() else {
        // No previous wallpaper was recorded: at least restore the normal fill
        // mode without touching the current image.
        set_registry("WallpaperStyle", "10")?;
        set_registry("TileWallpaper", "0")?;
        return Ok(());
    };
    set_registry("WallpaperStyle", &style)?;
    set_registry("TileWallpaper", &tile)?;
    if path.is_file() {
        apply(&path)
    } else {
        Ok(())
    }
}

fn wallpaper_image(path: &Path, lang: Lang) -> Result<PathBuf, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "jpg" | "jpeg" | "png" | "bmp") {
        return path
            .canonicalize()
            .map_err(|error| format!("wallpaper image: {error}"));
    }

    let decoded = crate::decode::decode(path, lang)?;
    let image = image::RgbaImage::from_raw(decoded.width, decoded.height, decoded.rgba)
        .ok_or("wallpaper image buffer is incomplete")?;
    let dir = wallpaper_dir()?;
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("create wallpaper folder: {error}"))?;
    let output = dir.join("current.png");
    image
        .save(&output)
        .map_err(|error| format!("save wallpaper image: {error}"))?;
    Ok(output)
}

fn wallpaper_dir() -> Result<PathBuf, String> {
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "LOCALAPPDATA is not set".to_owned())?;
    Ok(root.join("Bladr").join("Wallpaper"))
}

fn backup_path() -> Result<PathBuf, String> {
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "APPDATA is not set".to_owned())?;
    Ok(root.join("Bladr").join("wallpaper_backup.txt"))
}

fn backup() -> Result<(), String> {
    let path = backup_path()?;
    if path.is_file() {
        return Ok(());
    }
    let Some(wallpaper) = query_registry("WallPaper") else {
        return Ok(());
    };
    let style = query_registry("WallpaperStyle").unwrap_or_else(|| "10".to_owned());
    let tile = query_registry("TileWallpaper").unwrap_or_else(|| "0".to_owned());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create wallpaper backup folder: {error}"))?;
    }
    std::fs::write(&path, format!("{wallpaper}\n{style}\n{tile}\n"))
        .map_err(|error| format!("save wallpaper backup: {error}"))
}

fn read_backup() -> Option<(PathBuf, String, String)> {
    let text = std::fs::read_to_string(backup_path().ok()?).ok()?;
    let mut lines = text.lines();
    let path = PathBuf::from(lines.next()?.trim());
    let style = lines.next()?.trim().to_owned();
    let tile = lines.next()?.trim().to_owned();
    Some((path, style, tile))
}

fn query_registry(name: &str) -> Option<String> {
    let output = Command::new("reg")
        .args(["query", r"HKCU\Control Panel\Desktop", "/v", name])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().find(|line| line.contains(name))?;
    let value = line.split_once("REG_SZ").map(|(_, value)| value.trim())?;
    (!value.is_empty()).then(|| value.to_owned())
}

fn set_registry(name: &str, value: &str) -> Result<(), String> {
    let status = Command::new("reg")
        .args([
            "add",
            r"HKCU\Control Panel\Desktop",
            "/v",
            name,
            "/t",
            "REG_SZ",
            "/d",
            value,
            "/f",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|error| format!("update wallpaper setting: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("could not update wallpaper setting {name}"))
    }
}

fn apply(path: &Path) -> Result<(), String> {
    let path = path
        .canonicalize()
        .map_err(|error| format!("wallpaper path: {error}"))?;
    let mut text = path.display().to_string();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        text = format!(r"\\{rest}");
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        text = rest.to_owned();
    }
    let wide: Vec<u16> = std::ffi::OsStr::new(&text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            wide.as_ptr() as *mut c_void,
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    };
    if ok != 0 {
        Ok(())
    } else {
        Err(format!(
            "Windows rejected the wallpaper ({})",
            unsafe { GetLastError() }
        ))
    }
}

#[link(name = "user32")]
extern "system" {
    fn SystemParametersInfoW(
        action: u32,
        parameter: u32,
        value: *mut c_void,
        win_ini: u32,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetLastError() -> u32;
}
