//! Crop a region of the displayed image, copy it to the clipboard and save it.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn save_and_copy(
    width: u32,
    height: u32,
    rgba: Arc<Vec<u8>>,
    region: (u32, u32, u32, u32),
) -> Result<PathBuf, String> {
    let (x, y, region_width, region_height) = region;
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

    let image = image::RgbaImage::from_raw(region_width, region_height, cropped.clone())
        .ok_or("cropped image has an invalid size")?;
    let path = screenshot_path()?;
    image
        .save(&path)
        .map_err(|error| format!("save screenshot: {error}"))?;

    // Saving is the important part; clipboard access may be unavailable in a
    // restricted session, so report success if the file was written.
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_image(arboard::ImageData {
            width: region_width as usize,
            height: region_height as usize,
            bytes: Cow::Borrowed(&cropped),
        });
    }

    Ok(path)
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
