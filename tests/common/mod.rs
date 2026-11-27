//! Helpers shared by the integration tests: a minimal PNG reader and a hash.
#![allow(dead_code)]

pub mod inflate;
pub mod json;

/// A decoded PNG: width, height and RGBA8 pixels.
#[derive(Debug, PartialEq)]
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub color_type: u8,
    pub depth: u8,
    pub chunks: Vec<String>,
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in bytes {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    c ^ 0xffff_ffff
}

/// Reads the PNG files meshbox writes: not interlaced, indexed (1 to 8 bits) or RGBA8.
/// Checks every CRC and the zlib checksum.
pub fn decode_png(bytes: &[u8]) -> Decoded {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "signature");
    let mut pos = 8;
    let (mut width, mut height, mut depth, mut color_type) = (0, 0, 0, 0);
    let mut palette: Vec<[u8; 4]> = Vec::new();
    let mut idat = Vec::new();
    let mut chunks = Vec::new();
    loop {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        let kind = &bytes[pos + 4..pos + 8];
        let data = &bytes[pos + 8..pos + 8 + len];
        let crc = u32::from_be_bytes(bytes[pos + 8 + len..pos + 12 + len].try_into().unwrap());
        assert_eq!(crc, crc32(&bytes[pos + 4..pos + 8 + len]), "CRC");
        chunks.push(String::from_utf8(kind.to_vec()).unwrap());
        match kind {
            b"IHDR" => {
                width = u32::from_be_bytes(data[0..4].try_into().unwrap());
                height = u32::from_be_bytes(data[4..8].try_into().unwrap());
                depth = data[8];
                color_type = data[9];
                assert_eq!(&data[10..13], &[0, 0, 0], "no interlace");
            }
            b"PLTE" => {
                palette = data.chunks(3).map(|c| [c[0], c[1], c[2], 255]).collect();
            }
            b"tRNS" => {
                for (i, &a) in data.iter().enumerate() {
                    palette[i][3] = a;
                }
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            other => panic!("unexpected chunk {other:?}"),
        }
        pos += 12 + len;
    }
    assert_eq!(pos + 12, bytes.len(), "nothing after IEND");
    let raw = inflate::zlib_decompress(&idat, 1 << 30).expect("zlib");
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    match color_type {
        3 => {
            let stride = (width as usize * depth as usize).div_ceil(8);
            for row in raw.chunks(stride + 1) {
                assert_eq!(row[0], 0, "indexed rows are not filtered");
                for x in 0..width as usize {
                    let bit = x * depth as usize;
                    let byte = row[1 + bit / 8];
                    let shift = 8 - depth as usize - bit % 8;
                    let index = (byte >> shift) & ((1u16 << depth) - 1) as u8;
                    pixels.extend_from_slice(&palette[index as usize]);
                }
            }
        }
        6 => {
            assert_eq!(depth, 8);
            let stride = width as usize * 4;
            let mut previous = vec![0u8; stride];
            for row in raw.chunks(stride + 1) {
                let filter = row[0];
                let mut out = vec![0u8; stride];
                for i in 0..stride {
                    let a = if i >= 4 { out[i - 4] } else { 0 };
                    let b = previous[i];
                    let c = if i >= 4 { previous[i - 4] } else { 0 };
                    let p = match filter {
                        0 => 0,
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
                        f => panic!("filter {f}"),
                    };
                    out[i] = row[1 + i].wrapping_add(p);
                }
                pixels.extend_from_slice(&out);
                previous = out;
            }
        }
        t => panic!("color type {t}"),
    }
    assert_eq!(pixels.len(), width as usize * height as usize * 4);
    Decoded {
        width,
        height,
        pixels,
        color_type,
        depth,
        chunks,
    }
}

/// FNV-1a, 64 bits: a stable hash for golden tests.
pub fn fnv(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}
