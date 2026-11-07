//! Crop a region of the displayed image, then copy it or save it as PNG.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn copy(
    width: u32,
    height: u32,
    rgba: Arc<Vec<u8>>,
    region: (u32, u32, u32, u32),
) -> Result<(), String> {
    let (image_width, image_height, cropped) = crop(width, height, &rgba, region)?;
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|error| format!("open clipboard: {error}"))?;
    clipboard
        .set_image(arboard::ImageData {
            width: image_width as usize,
            height: image_height as usize,
            bytes: Cow::Borrowed(&cropped),
        })
        .map_err(|error| format!("copy to clipboard: {error}"))
}

pub fn save(
    width: u32,
    height: u32,
    rgba: Arc<Vec<u8>>,
    region: (u32, u32, u32, u32),
) -> Result<PathBuf, String> {
    let (image_width, image_height, cropped) = crop(width, height, &rgba, region)?;
    let image = image::RgbaImage::from_raw(image_width, image_height, cropped)
        .ok_or("cropped image has an invalid size")?;
    let path = screenshot_path()?;
    image
        .save(&path)
        .map_err(|error| format!("save screenshot: {error}"))?;
    Ok(path)
}

fn crop(
    width: u32,
    height: u32,
    rgba: &[u8],
    region: (u32, u32, u32, u32),
) -> Result<(u32, u32, Vec<u8>), String> {
    let (x, y, region_width, region_height) = region;
    if width == 0 || height == 0 {
        return Err("the image has no pixels".to_owned());
    }
    let x = x.min(width.saturating_sub(1));
    let y = y.min(height.saturating_sub(1));
    let region_width = region_width.min(width.saturating_sub(x)).max(1);
    let region_height = region_height.min(height.saturating_sub(y)).max(1);

    let mut cropped = Vec::with_capacity((region_width * region_height * 4) as usize);
    for row in y..y + region_height {
        let start = ((row * width + x) * 4) as usize;
        let end = start + (region_width * 4) as usize;
        if let Some(slice) = rgba.get(start..end) {
            cropped.extend_from_slice(slice);
        }
    }
    if cropped.len() != (region_width * region_height * 4) as usize {
        return Err("the selected area is outside the image".to_owned());
    }
    Ok((region_width, region_height, cropped))
}

fn screenshot_path() -> Result<PathBuf, String> {
    let root = std::env::var_os("BLADR_SCREENSHOT_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .map(|home| home.join("Pictures"))
        })
        .ok_or_else(|| "no Pictures folder found".to_owned())?;
    let dir = root.join("Bladr Screenshots");
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("create screenshot folder: {error}"))?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    for attempt in 0..100 {
        let name = if attempt == 0 {
            format!("Bladr-{stamp}.png")
        } else {
            format!("Bladr-{stamp}-{attempt}.png")
        };
        let path = dir.join(name);
        if !path.exists() {
            return Ok(path);
        }
    }
    Err("could not find a free screenshot filename".to_owned())
}
