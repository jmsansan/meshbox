mod common;

use meshbox::{Color, Image, png};

/// An image with `colors` distinct colors, some of them translucent.
fn image_with(colors: u32, width: u32, height: u32) -> Image {
    let mut image = Image::new(width, height).unwrap();
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) % colors;
            let a = if i % 5 == 1 { 128 } else { 255 };
            let c = Color::rgba((i * 37) as u8, (i * 11 + i / 256) as u8, (i / 7) as u8, a);
            image.set_pixel(x, y, c);
        }
    }
    image
}

#[test]
fn round_trips_every_color_count_bracket() {
    // 1, 2 colors: 1 bit; 3-4: 2 bits; 5-16: 4 bits; 17-256: 8 bits; more: RGBA.
    for (colors, color_type, depth) in [
        (1, 3, 1),
        (2, 3, 1),
        (3, 3, 2),
        (4, 3, 2),
        (5, 3, 4),
        (16, 3, 4),
        (17, 3, 8),
        (256, 3, 8),
        (257, 6, 8),
        (5000, 6, 8),
    ] {
        for (w, h) in [(1, 1), (7, 3), (33, 17), (100, 60)] {
            let image = image_with(colors, w, h);
            let bytes = png::encode(&image);
            let decoded = common::decode_png(&bytes);
            assert_eq!((decoded.width, decoded.height), (w, h));
            assert_eq!(decoded.pixels, image.pixels(), "{colors} colors, {w}x{h}");
            if w * h >= colors {
                assert_eq!(
                    (decoded.color_type, decoded.depth),
                    (color_type, depth),
                    "{colors} colors"
                );
            }
        }
    }
}

#[test]
fn same_image_same_bytes() {
    let image = image_with(300, 64, 64);
    assert_eq!(png::encode(&image), png::encode(&image));
    let small = image_with(6, 9, 9);
    assert_eq!(png::encode(&small), png::encode(&small));
}

#[test]
fn writes_no_metadata_chunks() {
    let decoded = common::decode_png(&png::encode(&image_with(6, 4, 4)));
    assert_eq!(decoded.chunks, ["IHDR", "PLTE", "tRNS", "IDAT", "IEND"]);
    let opaque = Image::new(2, 2).map(|mut i| {
        i.fill_rect(0, 0, 2, 2, Color::rgb(1, 2, 3));
        i
    });
    let decoded = common::decode_png(&png::encode(&opaque.unwrap()));
    assert_eq!(decoded.chunks, ["IHDR", "PLTE", "IDAT", "IEND"]);
}
