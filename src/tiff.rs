//! Minimal TIFF/DNG reader for the cases the `image` crate cannot handle.
//!
//! Many edited DNG files (Adobe "linear" DNGs) store a full-resolution,
//! uncompressed 16-bit RGB image instead of a JPEG preview. That image can be
//! displayed directly, without demosaicing. Camera-native DNGs still use CFA
//! data or JPEG compression and fall back to their embedded preview.

use std::borrow::Cow;
use std::collections::HashSet;
use std::io::Read;

use image::metadata::Orientation;
use image::{DynamicImage, ImageFormat, RgbImage};

const TAG_NEW_SUBFILE_TYPE: u16 = 254;
const TAG_WIDTH: u16 = 256;
const TAG_HEIGHT: u16 = 257;
const TAG_BITS: u16 = 258;
const TAG_COMPRESSION: u16 = 259;
const TAG_PHOTOMETRIC: u16 = 262;
const TAG_STRIP_OFFSETS: u16 = 273;
const TAG_ORIENTATION: u16 = 274;
const TAG_SAMPLES: u16 = 277;
const TAG_ROWS_PER_STRIP: u16 = 278;
const TAG_STRIP_COUNTS: u16 = 279;
const TAG_PLANAR: u16 = 284;
const TAG_TILE_WIDTH: u16 = 322;
const TAG_TILE_HEIGHT: u16 = 323;
const TAG_TILE_OFFSETS: u16 = 324;
const TAG_TILE_COUNTS: u16 = 325;
const TAG_SUB_IFDS: u16 = 330;
const TAG_JPEG_TABLES: u16 = 347;

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

struct Header {
    endian: Endian,
    first_ifd: u32,
}

struct Entry {
    typ: u16,
    count: u32,
    /// Offset of the 4-byte value field in the IFD entry.
    value_field: usize,
}

#[derive(Default, Clone)]
struct ImageInfo {
    width: u32,
    height: u32,
    bits: Vec<u16>,
    compression: u16,
    photometric: u16,
    samples: u16,
    planar: u16,
    orientation: u16,
    rows_per_strip: u32,
    strip_offsets: Vec<u32>,
    strip_counts: Vec<u32>,
    tile_width: u32,
    tile_height: u32,
    tile_offsets: Vec<u32>,
    tile_counts: Vec<u32>,
    jpeg_tables: Vec<u8>,
}

impl ImageInfo {
    fn is_decodable(&self) -> bool {
        let bits_ok = !self.bits.is_empty()
            && (self.bits.iter().all(|b| *b == 8) || self.bits.iter().all(|b| *b == 16));
        let layout_ok = !self.strip_offsets.is_empty() || !self.tile_offsets.is_empty();
        let compression_ok = matches!(self.compression, 1 | 5 | 8 | 32946)
            || (self.compression == 7 && self.bits.iter().all(|b| *b == 8));
        let photometric_ok = matches!(self.photometric, 2 | 6 | 34892);

        compression_ok
            && self.planar == 1
            && self.samples >= 3
            && bits_ok
            && photometric_ok
            && self.width > 0
            && self.height > 0
            && layout_ok
    }

    fn block_count(&self) -> usize {
        self.strip_offsets.len().max(self.tile_offsets.len())
    }
}

pub fn decode_largest_rgb(data: &[u8]) -> Option<DynamicImage> {
    let (header, infos) = collect_all(data)?;
    let mut candidates: Vec<ImageInfo> = infos.into_iter().filter(ImageInfo::is_decodable).collect();

    // Largest image first; on equal size prefer the one with fewer blocks
    // (a single full-size JPEG preview beats ~900 lossless RAW tiles).
    candidates.sort_by_key(|info| {
        (
            info.width as u64 * info.height as u64,
            std::cmp::Reverse(info.block_count()),
        )
    });
    candidates.reverse();

    for candidate in candidates {
        if let Some(image) = decode_info(data, &header, &candidate) {
            return Some(image);
        }
    }

    None
}

/// DNG 1.7 can store the main image as JPEG XL (compression 52546), which is
/// used by DxO PureRAW. The small TIFF reader cannot decode that, but knowing
/// it lets the viewer show a useful message instead of a tiny thumbnail.
pub fn uses_jpeg_xl(data: &[u8]) -> bool {
    const COMPRESSION_JXL_DNG_1_7: u16 = 52546;

    collect_all(data)
        .map(|(_, infos)| {
            infos
                .iter()
                .any(|info| info.compression == COMPRESSION_JXL_DNG_1_7)
        })
        .unwrap_or(false)
}

fn collect_all(data: &[u8]) -> Option<(Header, Vec<ImageInfo>)> {
    let header = parse_header(data)?;
    let mut infos = Vec::new();
    let mut visited = HashSet::new();
    let mut offset = header.first_ifd;

    while offset != 0 && visited.insert(offset) {
        let next = collect_ifd(data, &header, offset, 0, &mut infos, &mut visited).unwrap_or(0);
        offset = next;
    }

    Some((header, infos))
}

fn parse_header(data: &[u8]) -> Option<Header> {
    if data.len() < 8 {
        return None;
    }
    let endian = match &data[0..2] {
        b"II" => Endian::Little,
        b"MM" => Endian::Big,
        _ => return None,
    };
    if read_u16(data, 2, endian)? != 42 {
        return None;
    }
    Some(Header {
        endian,
        first_ifd: read_u32(data, 4, endian)?,
    })
}

fn collect_ifd(
    data: &[u8],
    header: &Header,
    offset: u32,
    depth: usize,
    out: &mut Vec<ImageInfo>,
    visited: &mut HashSet<u32>,
) -> Option<u32> {
    if depth > 8 {
        return None;
    }
    let offset = offset as usize;
    let count = read_u16(data, offset, header.endian)? as usize;
    let mut info = ImageInfo::default();
    let mut sub_ifds = Vec::new();
    let mut next_ifd = 0u32;

    for index in 0..count {
        let entry_at = offset + 2 + index * 12;
        if entry_at + 12 > data.len() {
            break;
        }
        let tag = read_u16(data, entry_at, header.endian)?;
        let typ = read_u16(data, entry_at + 2, header.endian)?;
        let value_count = read_u32(data, entry_at + 4, header.endian)?;
        let entry = Entry {
            typ,
            count: value_count,
            value_field: entry_at + 8,
        };

        match tag {
            TAG_NEW_SUBFILE_TYPE => {}
            TAG_WIDTH => info.width = first_value(data, header, &entry).unwrap_or(0),
            TAG_HEIGHT => info.height = first_value(data, header, &entry).unwrap_or(0),
            TAG_BITS => {
                info.bits = values(data, header, &entry)
                    .into_iter()
                    .map(|value| value as u16)
                    .collect()
            }
            TAG_COMPRESSION => info.compression = first_value(data, header, &entry).unwrap_or(0) as u16,
            TAG_PHOTOMETRIC => info.photometric = first_value(data, header, &entry).unwrap_or(0) as u16,
            TAG_STRIP_OFFSETS => info.strip_offsets = values(data, header, &entry),
            TAG_ORIENTATION => info.orientation = first_value(data, header, &entry).unwrap_or(1) as u16,
            TAG_SAMPLES => info.samples = first_value(data, header, &entry).unwrap_or(0) as u16,
            TAG_ROWS_PER_STRIP => {
                info.rows_per_strip = first_value(data, header, &entry).unwrap_or(0)
            }
            TAG_STRIP_COUNTS => info.strip_counts = values(data, header, &entry),
            TAG_PLANAR => info.planar = first_value(data, header, &entry).unwrap_or(1) as u16,
            TAG_TILE_WIDTH => info.tile_width = first_value(data, header, &entry).unwrap_or(0),
            TAG_TILE_HEIGHT => info.tile_height = first_value(data, header, &entry).unwrap_or(0),
            TAG_TILE_OFFSETS => info.tile_offsets = values(data, header, &entry),
            TAG_TILE_COUNTS => info.tile_counts = values(data, header, &entry),
            TAG_JPEG_TABLES => info.jpeg_tables = bytes_value(data, header, &entry),
            TAG_SUB_IFDS => sub_ifds = values(data, header, &entry),
            _ => {}
        }
    }

    out.push(info);

    for sub in sub_ifds {
        if sub != 0 && visited.insert(sub) {
            collect_ifd(data, header, sub, depth + 1, out, visited);
        }
    }

    let next_at = offset + 2 + count * 12;
    if next_at + 4 <= data.len() {
        next_ifd = read_u32(data, next_at, header.endian).unwrap_or(0);
    }
    Some(next_ifd)
}

fn bytes_value(data: &[u8], header: &Header, entry: &Entry) -> Vec<u8> {
    let count = entry.count as usize;
    if count == 0 {
        return Vec::new();
    }
    let width = match entry.typ {
        1 | 2 | 6 | 7 => 1usize,
        3 | 8 => 2,
        4 | 9 | 13 => 4,
        5 | 10 | 12 => 8,
        _ => 1,
    };
    let total = width.saturating_mul(count);
    let base = if total <= 4 {
        entry.value_field
    } else {
        read_u32(data, entry.value_field, header.endian).unwrap_or(0) as usize
    };
    data.get(base..base.saturating_add(total))
        .map(|slice| slice.to_vec())
        .unwrap_or_default()
}

fn values(data: &[u8], header: &Header, entry: &Entry) -> Vec<u32> {
    let count = entry.count as usize;
    let width = match entry.typ {
        1 | 2 | 6 | 7 => 1usize,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 => 8,
        _ => return Vec::new(),
    };
    if count == 0 {
        return Vec::new();
    }
    let total = width.saturating_mul(count);
    let base = if total <= 4 {
        entry.value_field
    } else {
        read_u32(data, entry.value_field, header.endian).unwrap_or(0) as usize
    };

    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let at = base + index * width;
        let value = match entry.typ {
            1 | 2 | 6 | 7 => data.get(at).copied().unwrap_or(0) as u32,
            3 | 8 => read_u16(data, at, header.endian).unwrap_or(0) as u32,
            4 | 9 | 13 => read_u32(data, at, header.endian).unwrap_or(0),
            11 => read_u32(data, at, header.endian).unwrap_or(0),
            _ => 0,
        };
        out.push(value);
    }
    out
}

fn first_value(data: &[u8], header: &Header, entry: &Entry) -> Option<u32> {
    values(data, header, entry).into_iter().next()
}

fn decode_info(data: &[u8], header: &Header, info: &ImageInfo) -> Option<DynamicImage> {
    if info.compression == 7 {
        return decode_jpeg_info(data, header, info);
    }

    let width = info.width as usize;
    let height = info.height as usize;
    let samples = info.samples as usize;
    let bytes = if info.bits.first().copied().unwrap_or(8) == 16 {
        2usize
    } else {
        1
    };
    let mut rgb = vec![0u8; width.checked_mul(height)?.checked_mul(3)?];

    if !info.strip_offsets.is_empty() {
        let rows_per_strip = if info.rows_per_strip == 0 {
            height as u32
        } else {
            info.rows_per_strip
        } as usize;

        for (index, &offset) in info.strip_offsets.iter().enumerate() {
            let y0 = index * rows_per_strip;
            if y0 >= height {
                break;
            }
            let rows = rows_per_strip.min(height - y0);
            let count = info
                .strip_counts
                .get(index)
                .copied()
                .unwrap_or(u32::MAX) as usize;
            let Some(block) = decompress(data, offset as usize, count, info.compression) else {
                continue;
            };
            copy_block(
                block.as_ref(),
                header.endian,
                width,
                samples,
                bytes,
                &mut rgb,
                width,
                height,
                y0,
                rows,
                0,
                width,
            );
        }
    } else {
        let tile_w = info.tile_width as usize;
        let tile_h = info.tile_height as usize;
        if tile_w == 0 || tile_h == 0 {
            return None;
        }
        let across = width.div_ceil(tile_w);
        let down = height.div_ceil(tile_h);

        for tile_y in 0..down {
            for tile_x in 0..across {
                let index = tile_y * across + tile_x;
                let Some(&offset) = info.tile_offsets.get(index) else {
                    continue;
                };
                let count = info
                    .tile_counts
                    .get(index)
                    .copied()
                    .unwrap_or(u32::MAX) as usize;
                let Some(block) = decompress(data, offset as usize, count, info.compression) else {
                    continue;
                };
                copy_block(
                    block.as_ref(),
                    header.endian,
                    tile_w,
                    samples,
                    bytes,
                    &mut rgb,
                    width,
                    height,
                    tile_y * tile_h,
                    tile_h,
                    tile_x * tile_w,
                    tile_w,
                );
            }
        }
    }

    let mut image = DynamicImage::ImageRgb8(RgbImage::from_raw(width as u32, height as u32, rgb)?);
    if let Some(orientation) = u8::try_from(info.orientation)
        .ok()
        .and_then(Orientation::from_exif)
    {
        image.apply_orientation(orientation);
    }
    Some(image)
}

fn decode_jpeg_info(data: &[u8], _header: &Header, info: &ImageInfo) -> Option<DynamicImage> {
    let width = info.width as usize;
    let height = info.height as usize;
    let mut rgb = vec![0u8; width.checked_mul(height)?.checked_mul(3)?];
    let mut decoded_any = false;

    if !info.strip_offsets.is_empty() {
        let rows_per_strip = if info.rows_per_strip == 0 {
            height as u32
        } else {
            info.rows_per_strip
        } as usize;

        for (index, &offset) in info.strip_offsets.iter().enumerate() {
            let y0 = index * rows_per_strip;
            if y0 >= height {
                break;
            }
            let rows = rows_per_strip.min(height - y0);
            let count = info
                .strip_counts
                .get(index)
                .copied()
                .unwrap_or(u32::MAX) as usize;
            let available = data.len().saturating_sub(offset as usize);
            let Some(block) = data.get(offset as usize..offset as usize + count.min(available))
            else {
                continue;
            };
            let Some(image) = decode_tiff_jpeg(block, &info.jpeg_tables) else {
                continue;
            };
            copy_decoded_block(&image, &mut rgb, width, height, 0, y0, width, rows);
            decoded_any = true;
        }
    } else {
        let tile_w = info.tile_width as usize;
        let tile_h = info.tile_height as usize;
        if tile_w == 0 || tile_h == 0 {
            return None;
        }
        let across = width.div_ceil(tile_w);
        let down = height.div_ceil(tile_h);

        for tile_y in 0..down {
            for tile_x in 0..across {
                let index = tile_y * across + tile_x;
                let Some(&offset) = info.tile_offsets.get(index) else {
                    continue;
                };
                let count = info
                    .tile_counts
                    .get(index)
                    .copied()
                    .unwrap_or(u32::MAX) as usize;
                let available = data.len().saturating_sub(offset as usize);
                let Some(block) = data.get(offset as usize..offset as usize + count.min(available))
                else {
                    continue;
                };
                let Some(image) = decode_tiff_jpeg(block, &info.jpeg_tables) else {
                    continue;
                };
                copy_decoded_block(
                    &image,
                    &mut rgb,
                    width,
                    height,
                    tile_x * tile_w,
                    tile_y * tile_h,
                    tile_w,
                    tile_h,
                );
                decoded_any = true;
            }
        }
    }

    if !decoded_any {
        return None;
    }

    let mut image = DynamicImage::ImageRgb8(RgbImage::from_raw(width as u32, height as u32, rgb)?);
    if let Some(orientation) = u8::try_from(info.orientation)
        .ok()
        .and_then(Orientation::from_exif)
    {
        image.apply_orientation(orientation);
    }
    Some(image)
}

/// Decode one TIFF JPEG strip/tile. Some writers put the shared Huffman and
/// quantization tables in tag 347 and store abbreviated streams in the blocks.
fn decode_tiff_jpeg(block: &[u8], tables: &[u8]) -> Option<DynamicImage> {
    if let Ok(image) = image::load_from_memory_with_format(block, ImageFormat::Jpeg) {
        return Some(image);
    }
    if tables.is_empty() {
        return None;
    }

    let mut combined = Vec::with_capacity(block.len() + tables.len() + 4);
    combined.extend_from_slice(&[0xFF, 0xD8]);
    let tables = tables.strip_prefix(&[0xFF, 0xD8]).unwrap_or(tables);
    let tables = tables.strip_suffix(&[0xFF, 0xD9]).unwrap_or(tables);
    combined.extend_from_slice(tables);
    let block = block.strip_prefix(&[0xFF, 0xD8]).unwrap_or(block);
    let block = block.strip_suffix(&[0xFF, 0xD9]).unwrap_or(block);
    combined.extend_from_slice(block);
    combined.extend_from_slice(&[0xFF, 0xD9]);

    image::load_from_memory_with_format(&combined, ImageFormat::Jpeg).ok()
}

#[allow(clippy::too_many_arguments)]
fn copy_decoded_block(
    image: &DynamicImage,
    rgb: &mut [u8],
    image_width: usize,
    image_height: usize,
    dest_x: usize,
    dest_y: usize,
    copy_width: usize,
    copy_height: usize,
) {
    let source = image.to_rgb8();
    let (source_width, source_height) = source.dimensions();
    let rows = copy_height
        .min(source_height as usize)
        .min(image_height.saturating_sub(dest_y));
    let columns = copy_width
        .min(source_width as usize)
        .min(image_width.saturating_sub(dest_x));

    for row in 0..rows {
        for column in 0..columns {
            let src = ((row * source_width as usize) + column) * 3;
            let dst = (((dest_y + row) * image_width) + dest_x + column) * 3;
            if src + 2 < source.as_raw().len() && dst + 2 < rgb.len() {
                rgb[dst..dst + 3].copy_from_slice(&source.as_raw()[src..src + 3]);
            }
        }
    }
}

fn decompress<'a>(
    data: &'a [u8],
    offset: usize,
    count: usize,
    compression: u16,
) -> Option<Cow<'a, [u8]>> {
    let available = data.len().saturating_sub(offset);
    let end = offset.checked_add(count.min(available))?;
    let raw = data.get(offset..end)?;

    match compression {
        1 => Some(Cow::Borrowed(raw)),
        5 => {
            let mut decoder =
                weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8);
            decoder.decode(raw).ok().map(Cow::Owned)
        }
        8 | 32946 => {
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(raw)
                .read_to_end(&mut out)
                .ok()?;
            Some(Cow::Owned(out))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn copy_block(
    block: &[u8],
    endian: Endian,
    block_width: usize,
    samples: usize,
    bytes: usize,
    rgb: &mut [u8],
    image_width: usize,
    image_height: usize,
    dest_y: usize,
    rows: usize,
    dest_x: usize,
    copy_width: usize,
) {
    let rows = rows.min(image_height.saturating_sub(dest_y));
    let copy_width = copy_width
        .min(block_width)
        .min(image_width.saturating_sub(dest_x));
    if rows == 0 || copy_width == 0 {
        return;
    }

    let block_end = block.len();
    let src_row_bytes = block_width.saturating_mul(samples).saturating_mul(bytes);

    for row in 0..rows {
        let src_row = row.saturating_mul(src_row_bytes);
        let dst_row = dest_y + row;
        for x in 0..copy_width {
            let src = src_row.saturating_add(x.saturating_mul(samples).saturating_mul(bytes));
            let dst = (dst_row * image_width + dest_x + x) * 3;
            if dst + 2 >= rgb.len() {
                break;
            }
            for channel in 0..3usize {
                let at = src.saturating_add(channel.saturating_mul(bytes));
                if at + bytes <= block_end {
                    rgb[dst + channel] = sample(block, at, bytes, endian);
                }
            }
        }
    }
}

fn sample(data: &[u8], at: usize, bytes: usize, endian: Endian) -> u8 {
    if bytes == 1 {
        data[at]
    } else {
        let value = match endian {
            Endian::Little => u16::from_le_bytes([data[at], data[at + 1]]),
            Endian::Big => u16::from_be_bytes([data[at], data[at + 1]]),
        };
        (value >> 8) as u8
    }
}

fn read_u16(data: &[u8], at: usize, endian: Endian) -> Option<u16> {
    let bytes = data.get(at..at + 2)?;
    Some(match endian {
        Endian::Little => u16::from_le_bytes([bytes[0], bytes[1]]),
        Endian::Big => u16::from_be_bytes([bytes[0], bytes[1]]),
    })
}

fn read_u32(data: &[u8], at: usize, endian: Endian) -> Option<u32> {
    let bytes = data.get(at..at + 4)?;
    Some(match endian {
        Endian::Little => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        Endian::Big => u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
    })
}
