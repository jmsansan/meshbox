//! PNG encoding, with the crate's own DEFLATE, zlib, CRC-32 and Adler-32.
//!
//! [`encode`] writes the smaller of two lossless forms: an indexed image (bit depth 1,
//! 2, 4 or 8, with a `tRNS` chunk for colors that are not opaque) when the image has at
//! most 256 distinct RGBA colors, and 8-bit RGBA otherwise. Rows of RGBA are filtered
//! with the filter that minimizes the sum of absolute differences. The file holds no
//! time stamp or other metadata, so the same image always gives the same bytes.
//!
//! ```
//! use meshbox::{Color, Image, png};
//!
//! let mut image = Image::new(2, 1)?;
//! image.set_pixel(0, 0, Color::rgb(255, 0, 0));
//! let bytes = png::encode(&image);
//! assert_eq!(&bytes[1..4], b"PNG");
//! # Ok::<(), meshbox::Error>(())
//! ```

mod checksum;
mod deflate;
#[cfg(test)]
mod inflate;

use std::collections::HashMap;
use std::path::Path;

use crate::{Image, Result};
use checksum::Crc32;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
/// The largest IDAT chunk written; larger data is split over several chunks.
const IDAT_SIZE: usize = 1 << 20;

/// Encodes `image` as a PNG file.
pub fn encode(image: &Image) -> Vec<u8> {
    let mut out = SIGNATURE.to_vec();
    let (width, height) = (image.width(), image.height());
    let palette = palette_of(image);
    let (header_depth, color_type, raw) = match &palette {
        Some((colors, indices)) => {
            let depth = match colors.len() {
                0..=2 => 1,
                3..=4 => 2,
                5..=16 => 4,
                _ => 8,
            };
            (depth, 3, indexed_rows(image, indices, depth))
        }
        None => (8, 6, filtered_rgba_rows(image)),
    };
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[header_depth, color_type, 0, 0, 0]);
    write_chunk(&mut out, b"IHDR", &header);
    if let Some((colors, _)) = &palette {
        let plte: Vec<u8> = colors.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
        write_chunk(&mut out, b"PLTE", &plte);
        let alphas = colors
            .iter()
            .rposition(|c| c[3] != 255)
            .map_or(0, |i| i + 1);
        if alphas > 0 {
            let trns: Vec<u8> = colors[..alphas].iter().map(|c| c[3]).collect();
            write_chunk(&mut out, b"tRNS", &trns);
        }
    }
    let compressed = deflate::zlib_compress(&raw);
    for chunk in compressed.chunks(IDAT_SIZE) {
        write_chunk(&mut out, b"IDAT", chunk);
    }
    write_chunk(&mut out, b"IEND", &[]);
    out
}

/// Encodes `image` and writes it to `path`.
pub fn write_file(path: impl AsRef<Path>, image: &Image) -> Result<()> {
    std::fs::write(path, encode(image))?;
    Ok(())
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc32::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

/// The image's colors in order of first appearance, and each pixel's index, if there
/// are at most 256 colors.
fn palette_of(image: &Image) -> Option<(Vec<[u8; 4]>, Vec<u8>)> {
    let mut colors = Vec::new();
    let mut lookup: HashMap<[u8; 4], u8> = HashMap::new();
    let mut indices = Vec::with_capacity(image.pixels().len() / 4);
    for p in image.pixels().chunks_exact(4) {
        let color = [p[0], p[1], p[2], p[3]];
        let index = match lookup.get(&color) {
            Some(&i) => i,
            None => {
                if colors.len() == 256 {
                    return None;
                }
                let i = colors.len() as u8;
                colors.push(color);
                lookup.insert(color, i);
                i
            }
        };
        indices.push(index);
    }
    Some((colors, indices))
}

fn indexed_rows(image: &Image, indices: &[u8], depth: u8) -> Vec<u8> {
    let width = image.width() as usize;
    let per_byte = 8 / depth as usize;
    let stride = width.div_ceil(per_byte);
    let mut raw = Vec::with_capacity((stride + 1) * image.height() as usize);
    for row in indices.chunks_exact(width) {
        raw.push(0);
        for group in row.chunks(per_byte) {
            let mut byte = 0u8;
            for (k, &index) in group.iter().enumerate() {
                byte |= index << (8 - depth as usize * (k + 1));
            }
            raw.push(byte);
        }
    }
    raw
}

fn filtered_rgba_rows(image: &Image) -> Vec<u8> {
    let stride = image.width() as usize * 4;
    let zero = vec![0u8; stride];
    let mut raw = Vec::with_capacity((stride + 1) * image.height() as usize);
    let mut candidate = vec![0u8; stride];
    let mut best = vec![0u8; stride];
    let mut previous: &[u8] = &zero;
    for row in image.pixels().chunks_exact(stride) {
        let mut best_filter = 0;
        let mut best_score = u64::MAX;
        for filter in 0..5u8 {
            for i in 0..stride {
                let a = if i >= 4 { row[i - 4] } else { 0 };
                let b = previous[i];
                let c = if i >= 4 { previous[i - 4] } else { 0 };
                candidate[i] = row[i].wrapping_sub(predict(filter, a, b, c));
            }
            let score: u64 = candidate
                .iter()
                .map(|&v| (v as i8).unsigned_abs() as u64)
                .sum();
            if score < best_score {
                best_score = score;
                best_filter = filter;
                std::mem::swap(&mut best, &mut candidate);
            }
        }
        raw.push(best_filter);
        raw.extend_from_slice(&best);
        previous = row;
    }
    raw
}

fn predict(filter: u8, a: u8, b: u8, c: u8) -> u8 {
    match filter {
        1 => a,
        2 => b,
        3 => ((a as u16 + b as u16) / 2) as u8,
        4 => {
            let p = a as i16 + b as i16 - c as i16;
            let (pa, pb, pc) = (
                (p - a as i16).abs(),
                (p - b as i16).abs(),
                (p - c as i16).abs(),
            );
            if pa <= pb && pa <= pc {
                a
            } else if pb <= pc {
                b
            } else {
                c
            }
        }
        _ => 0,
    }
}
