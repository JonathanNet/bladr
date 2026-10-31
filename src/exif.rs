//! Read the handful of EXIF fields shown in the HUD.
//!
//! This is deliberately a small TIFF/EXIF reader rather than a full metadata
//! stack: it handles JPEG APP1, TIFF-based RAW (NEF/DNG/CR2/ARW/…) and the
//! CMT1 box used by Canon CR3.

use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct ExifData {
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub exposure: Option<String>,
    pub aperture: Option<String>,
    pub iso: Option<u32>,
    pub focal: Option<String>,
    pub date: Option<String>,
    pub bias: Option<String>,
}

impl ExifData {
    pub fn is_empty(&self) -> bool {
        self.camera.is_none()
            && self.lens.is_none()
            && self.exposure.is_none()
            && self.aperture.is_none()
            && self.iso.is_none()
            && self.focal.is_none()
            && self.date.is_none()
            && self.bias.is_none()
    }

    /// Two or three compact lines for the HUD.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let mut exposure = Vec::new();
        if let Some(value) = &self.exposure {
            exposure.push(value.clone());
        }
        if let Some(value) = &self.aperture {
            exposure.push(value.clone());
        }
        if let Some(value) = self.iso {
            exposure.push(format!("ISO {value}"));
        }
        if let Some(value) = &self.focal {
            exposure.push(value.clone());
        }
        if let Some(value) = &self.bias {
            exposure.push(value.clone());
        }
        if !exposure.is_empty() {
            lines.push(exposure.join("   ·   "));
        }

        let mut camera = Vec::new();
        if let Some(value) = &self.camera {
            camera.push(value.clone());
        }
        if let Some(value) = &self.date {
            camera.push(value.clone());
        }
        if !camera.is_empty() {
            lines.push(camera.join("   ·   "));
        }

        if let Some(lens) = &self.lens {
            lines.push(lens.clone());
        }
        lines
    }
}

pub fn read(path: &Path) -> Option<ExifData> {
    let data = std::fs::read(path).ok()?;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if extension == "cr3" {
        return read_cr3(&data);
    }

    let base = if matches!(extension.as_str(), "jpg" | "jpeg" | "jpe" | "jfif") {
        find(&data, b"Exif\0\0")? + 6
    } else {
        0
    };

    let tiff = Tiff::new(&data, base)?;
    tiff.parse()
}

fn read_cr3(data: &[u8]) -> Option<ExifData> {
    let mut merged = ExifData::default();
    for name in [b"CMT1".as_slice(), b"CMT2".as_slice()] {
        let Some(at) = find(data, name) else {
            continue;
        };
        let Some(exif) = Tiff::new(data, at + 4).and_then(|tiff| tiff.parse()) else {
            continue;
        };
        merge(&mut merged, exif);
    }
    (!merged.is_empty()).then_some(merged)
}

fn merge(target: &mut ExifData, source: ExifData) {
    if source.camera.is_some() {
        target.camera = source.camera;
    }
    if source.lens.is_some() {
        target.lens = source.lens;
    }
    if source.exposure.is_some() {
        target.exposure = source.exposure;
    }
    if source.aperture.is_some() {
        target.aperture = source.aperture;
    }
    if source.iso.is_some() {
        target.iso = source.iso;
    }
    if source.focal.is_some() {
        target.focal = source.focal;
    }
    if source.date.is_some() {
        target.date = source.date;
    }
    if source.bias.is_some() {
        target.bias = source.bias;
    }
}

fn find(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|window| window == needle)
}

fn format_date(text: String) -> String {
    let Some(split) = text.find(' ') else {
        return text.replace(':', "-");
    };
    let (date, time) = text.split_at(split);
    format!("{} {}", date.replace(':', "-"), time.trim_start())
}

struct Entry {
    typ: u16,
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
        if (Self { data, base, little }).u16(base + 2)? != 42 {
            return None;
        }
        Some(Self { data, base, little })
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

    fn parse(&self) -> Option<ExifData> {
        let ifd0 = self.base.checked_add(self.u32(self.base + 4)? as usize)?;
        let mut tags = HashMap::new();
        self.read_ifd(ifd0, &mut tags);

        if let Some(entry) = tags.get(&34665) {
            if let Some(offset) = self.u32(entry.field) {
                let offset = self.base.checked_add(offset as usize)?;
                self.read_ifd(offset, &mut tags);
            }
        }

        let mut exif = ExifData::default();
        exif.camera = self.camera(&tags);
        exif.lens = self.ascii(&tags, 42036);
        exif.date = self.ascii(&tags, 36867).map(format_date);
        exif.exposure = self
            .rational(&tags, 33434)
            .map(|value| {
                if value > 0.0 && value < 1.0 {
                    format!("1/{:.0} s", 1.0 / value)
                } else if value >= 1.0 {
                    format!("{value:.1} s")
                } else {
                    format!("{value:.1} s")
                }
            });
        exif.aperture = self
            .rational(&tags, 33437)
            .map(|value| format!("f/{value:.1}"));
        exif.iso = self.short(&tags, 34855).map(u32::from);
        exif.focal = self
            .rational(&tags, 37386)
            .map(|value| format!("{value:.0} mm"));
        exif.bias = self
            .srational(&tags, 37380)
            .map(|value| format!("{value:+.1} EV"));

        (!exif.is_empty()).then_some(exif)
    }

    fn read_ifd(&self, offset: usize, tags: &mut HashMap<u16, Entry>) {
        let Some(count) = self.u16(offset) else {
            return;
        };
        for index in 0..count as usize {
            let entry = offset + 2 + index * 12;
            let (Some(tag), Some(typ), Some(count)) =
                (self.u16(entry), self.u16(entry + 2), self.u32(entry + 4))
            else {
                continue;
            };
            tags.insert(
                tag,
                Entry {
                    typ,
                    count,
                    field: entry + 8,
                },
            );
        }
    }

    fn value_range(&self, entry: &Entry) -> Option<(usize, usize)> {
        let unit = match entry.typ {
            1 | 2 | 6 | 7 => 1usize,
            3 | 8 => 2,
            4 | 9 | 11 => 4,
            5 | 10 | 12 => 8,
            _ => return None,
        };
        let total = unit.checked_mul(entry.count as usize)?;
        let start = if total <= 4 {
            entry.field
        } else {
            self.base.checked_add(self.u32(entry.field)? as usize)?
        };
        (start.checked_add(total)? <= self.data.len()).then_some((start, total))
    }

    fn ascii(&self, tags: &HashMap<u16, Entry>, tag: u16) -> Option<String> {
        let entry = tags.get(&tag)?;
        if entry.typ != 2 {
            return None;
        }
        let (start, total) = self.value_range(entry)?;
        let text = String::from_utf8_lossy(self.data.get(start..start + total)?);
        let text = text.trim_end_matches('\0').trim();
        (!text.is_empty()).then(|| text.to_owned())
    }

    fn camera(&self, tags: &HashMap<u16, Entry>) -> Option<String> {
        let make = self.ascii(tags, 271);
        let model = self.ascii(tags, 272);
        match (make, model) {
            (Some(make), Some(model)) => {
                if model.to_ascii_lowercase().contains(&make.to_ascii_lowercase()) {
                    Some(model)
                } else {
                    Some(format!("{make} {model}"))
                }
            }
            (None, Some(model)) => Some(model),
            (Some(make), None) => Some(make),
            _ => None,
        }
    }

    fn short(&self, tags: &HashMap<u16, Entry>, tag: u16) -> Option<u16> {
        let entry = tags.get(&tag)?;
        if entry.typ != 3 {
            return None;
        }
        let (start, _) = self.value_range(entry)?;
        self.u16(start)
    }

    fn rational(&self, tags: &HashMap<u16, Entry>, tag: u16) -> Option<f64> {
        let entry = tags.get(&tag)?;
        if entry.typ != 5 && entry.typ != 10 {
            return None;
        }
        let (start, _) = self.value_range(entry)?;
        let numerator = self.u32(start)?;
        let denominator = self.u32(start + 4)?;
        (denominator != 0).then_some(numerator as f64 / denominator as f64)
    }

    fn srational(&self, tags: &HashMap<u16, Entry>, tag: u16) -> Option<f64> {
        let entry = tags.get(&tag)?;
        if entry.typ != 10 && entry.typ != 5 {
            return None;
        }
        let (start, _) = self.value_range(entry)?;
        let numerator = self.u32(start)? as i32;
        let denominator = self.u32(start + 4)? as i32;
        (denominator != 0).then_some(numerator as f64 / denominator as f64)
    }
}
