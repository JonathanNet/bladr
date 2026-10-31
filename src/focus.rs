//! Canon AF point extraction for the "Show Focus Points" overlay.
//!
//! Canon's CR3 stores the AFInfo2 record in the CMT3 maker-note box. The
//! coordinates are relative to the image center, with positive Y upwards.

use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct FocusPoint {
    /// Normalized centre in the stored image orientation (0..1).
    pub x: f32,
    pub y: f32,
    /// Normalized size.
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug)]
pub struct FocusData {
    pub points: Vec<FocusPoint>,
}

pub fn read(path: &Path) -> Option<FocusData> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension != "cr3" {
        return None;
    }

    let data = std::fs::read(path).ok()?;
    let base = find(&data, b"CMT3")? + 4;
    let tiff = Tiff::new(&data, base)?;
    let tags = tiff.ifd0()?;
    let entry = tags.get(&38)?;
    let start = tiff.value_start(entry)?;
    parse_af_info2(&data, start, entry.count as usize)
}

fn parse_af_info2(data: &[u8], start: usize, count: usize) -> Option<FocusData> {
    if count < 8 {
        return None;
    }
    let value = |index: usize| -> Option<u16> {
        let at = start.checked_add(index.checked_mul(2)?)?;
        let bytes = data.get(at..at + 2)?;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    };
    let signed = |index: usize| -> Option<i16> {
        value(index).map(|v| v as i16)
    };

    let num = value(2)? as usize;
    let valid = value(3)? as usize;
    let image_width = value(6)? as u32;
    let image_height = value(7)? as u32;
    if num == 0 || num > 2000 || image_width == 0 || image_height == 0 {
        return None;
    }

    let widths_at = 8;
    let heights_at = widths_at + num;
    let xs_at = heights_at + num;
    let ys_at = xs_at + num;
    let focus_at = ys_at + num;
    let bits = ((num + 15) / 16).max(1);
    if focus_at + bits > count {
        return None;
    }

    let mut points = Vec::new();
    for index in 0..num.min(valid) {
        let word = value(focus_at + index / 16)?;
        if word & (1 << (index % 16)) == 0 {
            continue;
        }
        let width = signed(widths_at + index)? as f32;
        let height = signed(heights_at + index)? as f32;
        let x = signed(xs_at + index)? as f32;
        let y = signed(ys_at + index)? as f32;
        if width <= 0.0 || height <= 0.0 {
            continue;
        }

        let cx = 0.5 + x / image_width as f32;
        let cy = 0.5 - y / image_height as f32;
        points.push(FocusPoint {
            x: cx.clamp(0.0, 1.0),
            y: cy.clamp(0.0, 1.0),
            width: (width / image_width as f32).clamp(0.0, 1.0),
            height: (height / image_height as f32).clamp(0.0, 1.0),
        });
    }

    (!points.is_empty()).then_some(FocusData { points })
}

fn find(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|window| window == needle)
}

struct Entry {
    count: u32,
    field: usize,
}

struct Tiff<'a> {
    data: &'a [u8],
    base: usize,
    little: bool,
}

impl<'a> Tiff<'a> {
    fn new(data: &'a [u8], base: usize) -> Option<Self> {
        if base + 8 > data.len() {
            return None;
        }
        let little = match &data[base..base + 2] {
            b"II" => true,
            b"MM" => false,
            _ => return None,
        };
        let tiff = Self { data, base, little };
        (tiff.u16(base + 2)? == 42).then_some(tiff)
    }

    fn u16(&self, offset: usize) -> Option<u16> {
        let bytes = self.data.get(offset..offset + 2)?;
        Some(if self.little {
            u16::from_le_bytes([bytes[0], bytes[1]])
        } else {
            u16::from_be_bytes([bytes[0], bytes[1]])
        })
    }

    fn u32(&self, offset: usize) -> Option<u32> {
        let bytes = self.data.get(offset..offset + 4)?;
        Some(if self.little {
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        } else {
            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        })
    }

    fn ifd0(&self) -> Option<HashMap<u16, Entry>> {
        let offset = self.base.checked_add(self.u32(self.base + 4)? as usize)?;
        let count = self.u16(offset)?;
        let mut tags = HashMap::new();
        for index in 0..count as usize {
            let at = offset + 2 + index * 12;
            let (Some(tag), Some(_typ), Some(count)) =
                (self.u16(at), self.u16(at + 2), self.u32(at + 4))
            else {
                continue;
            };
            tags.insert(
                tag,
                Entry {
                    count,
                    field: at + 8,
                },
            );
        }
        Some(tags)
    }

    fn value_start(&self, entry: &Entry) -> Option<usize> {
        let unit = 2usize; // AFInfo2 is stored as an int16u sequence
        let total = unit.checked_mul(entry.count as usize)?;
        if total <= 4 {
            Some(entry.field)
        } else {
            self.base.checked_add(self.u32(entry.field)? as usize)
        }
    }
}
