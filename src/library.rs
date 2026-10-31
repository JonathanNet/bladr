//! Folder scanning and the natural, Explorer-like ordering used for browsing.

use std::cmp::Ordering;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::i18n::{self, Lang};

/// Extensions the bundled `image` codecs can decode, plus the RAW containers
/// that are shown through their embedded JPEG preview.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "gif", "bmp", "webp", "tif", "tiff", "ico", "tga", "dds",
    "hdr", "pnm", "pbm", "pgm", "ppm", "qoi", "ff", "farbfeld",
    // RAW (embedded preview)
    "cr3", "cr2", "crw", "nef", "nrw", "arw", "srf", "sr2", "dng", "orf", "rw2", "raf", "pef",
    "srw", "rwl", "3fr", "erf", "mef", "mos", "mrw", "x3f", "iiq", "k25", "kdc", "dcr",
];

#[derive(Clone, Debug)]
pub struct Photo {
    pub path: PathBuf,
    pub name: String,
    /// File size in bytes, when the directory entry reported one.
    pub size: u64,
}

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_ascii_lowercase();
            SUPPORTED_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

/// Read every supported image directly inside `dir` and sort it naturally.
///
/// Subfolders are deliberately ignored: a viewer that blurs into the whole
/// tree is surprising when you point it at a folder of exports.
pub fn scan_folder(dir: &Path) -> io::Result<Vec<Photo>> {
    let mut photos = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();

        // `file_type` avoids following directory symlinks and keeps this cheap.
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if !file_type.is_file() || !is_supported(&path) {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }

        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        photos.push(Photo { path, name, size });
    }

    photos.sort_by(|a, b| {
        natural_cmp(&a.name, &b.name).then_with(|| a.name.cmp(&b.name))
    });
    Ok(photos)
}

/// Locate `wanted` inside an already scanned list.
pub fn index_of(photos: &[Photo], wanted: &Path) -> Option<usize> {
    // Compare canonical paths when possible; UNC paths are often returned in
    // both `\\server\share` and `\\?\UNC\server\share` form, so compare a
    // normalised form as well. Windows paths are case-insensitive.
    let canonical = fs::canonicalize(wanted).ok();
    photos.iter().position(|p| {
        path_eq(&p.path, wanted)
            || canonical
                .as_deref()
                .map(|c| path_eq(&p.path, c))
                .unwrap_or(false)
    })
}

fn path_eq(a: &Path, b: &Path) -> bool {
    a == b || normalise_path(a) == normalise_path(b)
}

fn normalise_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    let text = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        text.into_owned()
    };
    text.replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// Turn a raw `io::Error` into a message that is useful when a network share
/// is involved. Windows error 5/1326 is by far the most common one here.
pub fn describe_scan_error(dir: &Path, err: &io::Error, lang: Lang) -> String {
    let mut message = i18n::tf(
        lang,
        "scan.open_failed",
        &[
            ("path", &dir.display().to_string()),
            ("error", &err.to_string()),
        ],
    );
    let suffix = match err.raw_os_error() {
        Some(5) | Some(1326) => i18n::tr(lang, "scan.denied"),
        Some(53) | Some(67) | Some(3) => i18n::tr(lang, "scan.not_found"),
        Some(64) => i18n::tr(lang, "scan.aborted"),
        Some(1231) => i18n::tr(lang, "scan.unavailable"),
        _ => i18n::tr(lang, "scan.tip"),
    };
    message.push_str("\n\n");
    message.push_str(suffix);
    message
}

/// Compare strings the way Explorer does: `img2` sorts before `img10`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();
    let ab = a_lower.as_bytes();
    let bb = b_lower.as_bytes();

    let (mut i, mut j) = (0usize, 0usize);
    while i < ab.len() && j < bb.len() {
        let a_digit = ab[i].is_ascii_digit();
        let b_digit = bb[j].is_ascii_digit();

        if a_digit && b_digit {
            let a_start = i;
            while i < ab.len() && ab[i].is_ascii_digit() {
                i += 1;
            }
            let b_start = j;
            while j < bb.len() && bb[j].is_ascii_digit() {
                j += 1;
            }

            let a_num = a_lower[a_start..i].trim_start_matches('0');
            let b_num = b_lower[b_start..j].trim_start_matches('0');

            let ord = a_num
                .len()
                .cmp(&b_num.len())
                .then_with(|| a_num.cmp(b_num));
            if ord != Ordering::Equal {
                return ord;
            }
        } else {
            // Advance whole UTF-8 characters so slicing stays on boundaries.
            let a_char = a_lower[i..].chars().next().unwrap();
            let b_char = b_lower[j..].chars().next().unwrap();
            let ord = a_char.cmp(&b_char);
            if ord != Ordering::Equal {
                return ord;
            }
            i += a_char.len_utf8();
            j += b_char.len_utf8();
        }
    }

    ab.len().cmp(&bb.len())
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
