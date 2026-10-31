//! Background decoding.
//!
//! One worker keeps disk and CPU contention predictable while the UI stays
//! responsive. The UI publishes the small set of indices it actually wants,
//! so requests that were queued before a fast scroll are skipped instead of
//! turning into a backlog.
//!
//! RAW files (CR3, NEF, ARW, …) are shown through the camera's embedded JPEG
//! preview. Most modern cameras embed a full-resolution preview, so this gives
//! a sharp viewer image without a full RAW demosaic pipeline.

use std::collections::HashSet;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader};

use crate::i18n::{self, Lang};

/// Cap the GPU texture so a 100 MP scan does not allocate a 400 MB surface.
/// The full resolution is still reported in the info bar.
const MAX_TEXTURE_SIDE: u32 = 4096;

pub const RAW_EXTENSIONS: &[&str] = &[
    "cr3", "cr2", "crw", "nef", "nrw", "arw", "srf", "sr2", "dng", "orf", "rw2", "raf", "pef",
    "srw", "rwl", "3fr", "erf", "mef", "mos", "mrw", "x3f", "iiq", "k25", "kdc", "dcr",
];

#[derive(Clone)]
pub struct Job {
    pub index: usize,
    pub path: PathBuf,
}

pub struct Decoded {
    pub index: usize,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Dimensions after EXIF orientation, before the texture cap.
    pub source_width: u32,
    pub source_height: u32,
    /// True when the pixels came from the RAW file's embedded preview.
    pub raw: bool,
}

pub enum DecodeEvent {
    Loaded(Decoded),
    Failed { index: usize, message: String },
}

#[derive(Default)]
struct Shared {
    wanted: HashSet<usize>,
    inflight: HashSet<usize>,
}

pub struct Decoder {
    tx: Sender<Job>,
    rx: Receiver<DecodeEvent>,
    shared: Arc<Mutex<Shared>>,
}

impl Decoder {
    pub fn spawn() -> Self {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (event_tx, event_rx) = mpsc::channel::<DecodeEvent>();
        let shared = Arc::new(Mutex::new(Shared::default()));

        let worker_shared = Arc::clone(&shared);
        thread::Builder::new()
            .name("bladr-decode".to_owned())
            .spawn(move || worker(job_rx, event_tx, worker_shared))
            .expect("failed to spawn decoder thread");

        Self {
            tx: job_tx,
            rx: event_rx,
            shared,
        }
    }

    /// Replace the wanted set. Anything else in the queue becomes stale.
    pub fn set_wanted(&self, wanted: impl IntoIterator<Item = usize>) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.wanted.clear();
            shared.wanted.extend(wanted);
        }
    }

    /// Queue `index` unless it is already being decoded.
    pub fn request(&self, index: usize, path: PathBuf) {
        let should_send = {
            match self.shared.lock() {
                Ok(mut shared) => {
                    if shared.inflight.contains(&index) {
                        false
                    } else {
                        shared.inflight.insert(index);
                        true
                    }
                }
                Err(_) => false,
            }
        };
        if should_send && self.tx.send(Job { index, path }).is_err() {
            if let Ok(mut shared) = self.shared.lock() {
                shared.inflight.remove(&index);
            }
        }
    }

    pub fn try_recv(&self) -> Option<DecodeEvent> {
        self.rx.try_recv().ok()
    }
}

fn worker(jobs: Receiver<Job>, events: Sender<DecodeEvent>, shared: Arc<Mutex<Shared>>) {
    while let Ok(job) = jobs.recv() {
        let wanted = shared
            .lock()
            .map(|s| s.wanted.contains(&job.index))
            .unwrap_or(false);

        if !wanted {
            if let Ok(mut s) = shared.lock() {
                s.inflight.remove(&job.index);
            }
            continue;
        }

        let result = decode(&job.path, i18n::decoder_lang());

        if let Ok(mut s) = shared.lock() {
            s.inflight.remove(&job.index);
        }

        let event = match result {
            Ok(mut decoded) => {
                decoded.index = job.index;
                DecodeEvent::Loaded(decoded)
            }
            Err(message) => DecodeEvent::Failed {
                index: job.index,
                message,
            },
        };
        if events.send(event).is_err() {
            break;
        }
    }
}

fn decode(path: &Path, lang: Lang) -> Result<Decoded, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if is_raw_path(path) {
        // Edited "linear" DNGs often store a full-size uncompressed RGB image
        // instead of a JPEG preview. That image is display-ready and beats a
        // small thumbnail, so try it first.
        if extension == "dng" {
            if let Ok(data) = fs::read(path) {
                let tiff_image = crate::tiff::decode_largest_rgb(&data);
                let tiff_area = tiff_image
                    .as_ref()
                    .map(|image| image.width() as u64 * image.height() as u64)
                    .unwrap_or(0);
                if tiff_area >= 1_500_000 {
                    return finish(tiff_image.expect("tiff image"), true);
                }

                return match decode_raw_preview_data(&data, lang) {
                    Ok(decoded) => Ok(decoded),
                    Err(error) => {
                        if crate::tiff::uses_jpeg_xl(&data) {
                            return Err(i18n::tr(lang, "decode.jpeg_xl").to_owned());
                        }
                        match tiff_image {
                            Some(image) => finish(image, true),
                            None => Err(error),
                        }
                    }
                };
            }
        }
        return decode_raw_preview(path, lang);
    }

    match decode_regular(path, lang) {
        Ok(decoded) => Ok(decoded),
        Err(error) => {
            // Some 16-bit RGB TIFFs are not handled by the image crate. The
            // small TIFF reader covers those uncompressed variants.
            if extension == "tif" || extension == "tiff" {
                if let Ok(data) = fs::read(path) {
                    if let Some(image) = crate::tiff::decode_largest_rgb(&data) {
                        return finish(image, false);
                    }
                }
            }
            Err(error)
        }
    }
}

pub fn is_raw_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| RAW_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn decode_regular(path: &Path, lang: Lang) -> Result<Decoded, String> {
    let reader = ImageReader::open(path)
        .map_err(|e| i18n::tf(lang, "decode.open", &[("e", &e.to_string())]))?
        .with_guessed_format()
        .map_err(|e| i18n::tf(lang, "decode.format", &[("e", &e.to_string())]))?;

    let mut decoder = reader
        .into_decoder()
        .map_err(|e| i18n::tf(lang, "decode.read", &[("e", &e.to_string())]))?;

    // Cameras store the sensor orientation in EXIF; honour it or every
    // portrait shot opens sideways.
    let orientation = decoder
        .orientation()
        .unwrap_or(Orientation::NoTransforms);

    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|e| i18n::tf(lang, "decode.decode", &[("e", &e.to_string())]))?;
    image.apply_orientation(orientation);

    finish(image, false)
}

fn finish(mut image: DynamicImage, raw: bool) -> Result<Decoded, String> {
    let source_width = image.width();
    let source_height = image.height();

    if source_width.max(source_height) > MAX_TEXTURE_SIDE {
        image = image.resize(MAX_TEXTURE_SIDE, MAX_TEXTURE_SIDE, FilterType::Lanczos3);
    }

    let rgba_image = image.to_rgba8();
    let (width, height) = (rgba_image.width(), rgba_image.height());

    Ok(Decoded {
        index: 0, // filled in by the caller
        rgba: rgba_image.into_raw(),
        width,
        height,
        source_width,
        source_height,
        raw,
    })
}

/// Decode a RAW file through the largest embedded JPEG preview.
///
/// This is a viewer, not a converter: the preview is what the camera itself
/// wrote, so no demosaicing or colour profile work is attempted. Canon CR3 and
/// Nikon NEF both normally carry a full-resolution JPEG.
fn decode_raw_preview(path: &Path, lang: Lang) -> Result<Decoded, String> {
    let data = fs::read(path)
        .map_err(|e| i18n::tf(lang, "decode.read_raw", &[("e", &e.to_string())]))?;
    decode_raw_preview_data(&data, lang)
}

fn decode_raw_preview_data(data: &[u8], lang: Lang) -> Result<Decoded, String> {
    let candidates = jpeg_previews(data);
    if candidates.is_empty() {
        return Err(i18n::tr(lang, "decode.no_preview").to_owned());
    }

    // Rank by decoded dimensions, not byte length. RAW containers also contain
    // lossless-JPEG tiles from the sensor image; those are large but cannot be
    // decoded, and must not crowd out the real preview.
    let mut candidates = candidates;
    candidates.sort_by_key(|preview| std::cmp::Reverse(preview.width as u64 * preview.height as u64));
    let candidate_count = candidates.len();

    let container_orientation = raw_orientation(data).unwrap_or(Orientation::NoTransforms);
    let mut best: Option<(u64, DynamicImage, Orientation)> = None;

    for preview in candidates.into_iter().take(16) {
        let Ok((width, height, image, orientation)) = decode_jpeg(preview.bytes) else {
            continue;
        };
        let area = width as u64 * height as u64;

        if best.as_ref().map(|(a, _, _)| area > *a).unwrap_or(true) {
            best = Some((area, image, orientation));
        }

        // A preview of at least ~3 MP is good enough to stop looking.
        if area >= 3_000_000 {
            break;
        }
    }

    let Some((_, mut image, preview_orientation)) = best else {
        return Err(i18n::tf(
            lang,
            "decode.preview_fail",
            &[("n", &candidate_count.to_string())],
        ));
    };

    // Prefer the preview's own EXIF orientation; fall back to the orientation
    // stored in the RAW container (NEF's TIFF IFD0 or CR3's CMT1 box).
    let orientation = if preview_orientation != Orientation::NoTransforms {
        preview_orientation
    } else {
        container_orientation
    };
    image.apply_orientation(orientation);

    finish(image, true)
}

fn decode_jpeg(bytes: &[u8]) -> Result<(u32, u32, DynamicImage, Orientation), String> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder
        .orientation()
        .unwrap_or(Orientation::NoTransforms);
    let image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    Ok((image.width(), image.height(), image, orientation))
}

struct Preview<'a> {
    bytes: &'a [u8],
    width: u32,
    height: u32,
}

/// Collect complete, decodable JPEG streams from the RAW container.
///
/// Lossless JPEG (SOF3) is used for some RAW tiles; `image` cannot decode it,
/// so those streams are ignored instead of filling the candidate list.
fn jpeg_previews(data: &[u8]) -> Vec<Preview<'_>> {
    let mut previews = Vec::new();
    let mut offset = 0usize;

    while offset + 2 < data.len() {
        if data[offset] == 0xFF && data[offset + 1] == 0xD8 {
            if let Some(end) = jpeg_end(data, offset) {
                let bytes = &data[offset..end];
                if let Some((width, height, sof)) = jpeg_dimensions(bytes) {
                    if matches!(sof, 0xC0 | 0xC1 | 0xC2) && width > 0 && height > 0 {
                        previews.push(Preview {
                            bytes,
                            width,
                            height,
                        });
                    }
                }
                offset = end;
                continue;
            }
        }
        offset += 1;
    }

    previews
}

/// Read width, height and SOF marker from a JPEG header.
fn jpeg_dimensions(data: &[u8]) -> Option<(u32, u32, u8)> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2usize;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        if marker == 0xD8 || marker == 0xD9 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if i + 3 >= data.len() {
            return None;
        }
        let length = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if length < 2 {
            return None;
        }
        if (0xC0..=0xCF).contains(&marker)
            && marker != 0xC4
            && marker != 0xC8
            && marker != 0xCC
            && i + 8 < data.len()
        {
            let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            return Some((width, height, marker));
        }
        i += 2 + length;
    }
    None
}

/// Walk JPEG markers and return the byte just after EOI.
fn jpeg_end(data: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 2;

    while i + 1 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        match marker {
            0xD9 => return Some(i + 2),           // EOI
            0xD8 => {
                i += 2;
            }                                      // nested SOI
            0x00 | 0xD0..=0xD7 | 0x01 => {
                i += 2;
            }                                      // stuffed / RST / TEM
            0xFF => {
                i += 1;
            }                                      // fill byte
            _ => {
                if i + 3 >= data.len() {
                    return None;
                }
                let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
                if len < 2 {
                    return None;
                }
                i += 2 + len;

                if marker == 0xDA {
                    // Start of scan: skip entropy-coded bytes until a real marker.
                    while i + 1 < data.len() {
                        if data[i] == 0xFF {
                            let next = data[i + 1];
                            if next == 0x00 || (0xD0..=0xD7).contains(&next) {
                                i += 2;
                                continue;
                            }
                            break;
                        }
                        i += 1;
                    }
                }
            }
        }
    }

    None
}

/// Read the EXIF orientation from a TIFF-based RAW container or Canon CMT1.
fn raw_orientation(data: &[u8]) -> Option<Orientation> {
    if data.len() > 8 && (data.starts_with(b"II*\0") || data.starts_with(b"MM\0*")) {
        if let Some(value) = tiff_orientation(data, 0) {
            return u8::try_from(value).ok().and_then(Orientation::from_exif);
        }
    }

    // Canon CR3: CMT1 is a TIFF/EXIF box inside the BMFF container.
    let mut offset = 0usize;
    while let Some(pos) = find_bytes(&data[offset..], b"CMT1") {
        let base = offset + pos + 4;
        if base + 8 <= data.len() {
            if let Some(value) = tiff_orientation(data, base) {
                return u8::try_from(value).ok().and_then(Orientation::from_exif);
            }
        }
        offset = base;
    }

    None
}

fn tiff_orientation(data: &[u8], base: usize) -> Option<u16> {
    if base + 8 > data.len() {
        return None;
    }
    let endian = match &data[base..base + 2] {
        b"II" => "little",
        b"MM" => "big",
        _ => return None,
    };
    let u16_at = |at: usize| -> Option<u16> {
        let bytes = data.get(at..at + 2)?;
        Some(if endian == "little" {
            u16::from_le_bytes([bytes[0], bytes[1]])
        } else {
            u16::from_be_bytes([bytes[0], bytes[1]])
        })
    };
    let u32_at = |at: usize| -> Option<u32> {
        let bytes = data.get(at..at + 4)?;
        Some(if endian == "little" {
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        } else {
            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        })
    };

    if u16_at(base + 2)? != 42 {
        return None;
    }
    let ifd = base + u32_at(base + 4)? as usize;
    let count = u16_at(ifd)? as usize;

    for index in 0..count {
        let entry = ifd + 2 + index * 12;
        if entry + 12 > data.len() {
            break;
        }
        let tag = u16_at(entry)?;
        if tag != 0x0112 {
            continue;
        }
        let kind = u16_at(entry + 2)?;
        let value_count = u32_at(entry + 4)?;
        if value_count == 0 {
            return None;
        }
        return match kind {
            3 => u16_at(entry + 8),
            4 => u32_at(entry + 8).map(|v| v as u16),
            _ => None,
        };
    }

    None
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
