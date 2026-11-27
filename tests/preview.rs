mod common;

use meshbox::{Color, Document, ErrorCode, Image, PreviewOptions, png};

/// The sheet's layout: an 8-pixel margin, a 7-pixel label and 4 pixels under it, and
/// 8 pixels between views.
const TOP: u32 = 8 + 7 + 4;

fn preview(text: &str, scale: u32) -> Image {
    Document::parse(text)
        .unwrap()
        .preview(&PreviewOptions { scale, grid: false })
        .unwrap()
}

fn pixel(image: &Image, x: u32, y: u32) -> Color {
    image.pixel(x, y).unwrap()
}

#[test]
fn a_red_voxel_is_a_square_in_each_axis_view() {
    let image = preview(
        "palette p\n  r #ff0000\nvoxels dot 1x1x1 palette=p\n  level 0\n    r\n",
        8,
    );
    // Front, right and top views: an 8 × 8 square each, 8 pixels apart.
    let lefts = [8, 24, 40];
    let mut colors = Vec::new();
    for &left in &lefts {
        let c = pixel(&image, left, TOP);
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(pixel(&image, left + x, TOP + y), c, "view at {left}");
            }
        }
        // Red, lit: no green or blue.
        assert!(c.r > 0 && c.g == 0 && c.b == 0, "{c:?}");
        // Nothing of the model outside the square: the next column is the gap.
        assert_ne!(pixel(&image, left + 8, TOP), c);
        colors.push(c);
    }
    let (front, right, top) = (colors[0], colors[1], colors[2]);
    // The top face is brighter than the sides, and the two sides differ.
    assert!(top.r > front.r && top.r > right.r, "{colors:?}");
    assert_ne!(front.r, right.r);
}

#[test]
fn hidden_faces_are_hidden() {
    // A large blue block, and a small red cube in front of it, bottom center.
    let text = "palette p\n  b #0000ff\n  r #ff0000\nvoxels m 3x3x2 palette=p\n  level 0\n    bbb\n    bbb\n    .r.\n  level 1\n    bbb\n    bbb\n    ...\n";
    let image = preview(text, 4);
    // Front view: 3 × 2 voxels of 4 pixels, its top left at (8, TOP).
    let at = |vx: u32, vy: u32| pixel(&image, 8 + vx * 4 + 2, TOP + vy * 4 + 2);
    let red = at(1, 1);
    assert!(
        red.r > 0 && red.b == 0,
        "the small cube hides the block: {red:?}"
    );
    for (vx, vy) in [(0, 0), (1, 0), (2, 0), (0, 1), (2, 1)] {
        let c = at(vx, vy);
        assert!(c.b > 0 && c.r == 0, "({vx}, {vy}): {c:?}");
    }
}

#[test]
fn the_top_view_reads_like_the_text() {
    // Level 0 written with a red voxel in the back-left corner: the top view shows it
    // at its top left, as the text does.
    let text = "palette p\n  r #ff0000\n  w #ffffff\nvoxels m 2x2x1 palette=p\n  level 0\n    rw\n    ww\n";
    let image = preview(text, 4);
    // Front 8 px wide, right 8 px wide: the top view starts at 8 + 8 + 8 + 8 + 8.
    let left = 40;
    let c = pixel(&image, left + 1, TOP + 1);
    assert!(c.r > 0 && c.g == 0, "{c:?}");
    let c = pixel(&image, left + 5, TOP + 5);
    assert!(c.g > 0, "{c:?}");
}

#[test]
fn translucent_voxels_show_what_is_behind_them() {
    let text = "palette p\n  r #ff0000\n  g #ffffff40\nvoxels m 1x2x1 palette=p\n  level 0\n    r\n    g\n";
    let image = preview(text, 4);
    // Front view: the glass in front of the red voxel lets the red through.
    let c = pixel(&image, 10, TOP + 2);
    assert!(c.r > c.g && c.g > 0, "{c:?}");
}

#[test]
fn the_grid_marks_voxel_boundaries() {
    let text = "palette p\n  w #ffffff\nvoxels m 2x2x1 palette=p\n  level 0\n    ww\n    ww\n";
    let doc = Document::parse(text).unwrap();
    let with = doc
        .preview(&PreviewOptions {
            scale: 4,
            grid: true,
        })
        .unwrap();
    let without = doc
        .preview(&PreviewOptions {
            scale: 4,
            grid: false,
        })
        .unwrap();
    let left = 40;
    // A boundary pixel is darker with the grid; a voxel's inside is the same.
    assert!(pixel(&with, left + 4, TOP + 1).r < pixel(&without, left + 4, TOP + 1).r);
    assert_eq!(
        pixel(&with, left + 2, TOP + 2),
        pixel(&without, left + 2, TOP + 2)
    );
}

#[test]
fn scale_is_checked() {
    let doc = Document::parse("palette p\n  w #ffffff\n").unwrap();
    for scale in [0, 33] {
        let err = doc
            .preview(&PreviewOptions { scale, grid: true })
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidArgument);
    }
    // No models: an empty sheet.
    let empty = doc.preview(&PreviewOptions::default()).unwrap();
    assert_eq!((empty.width(), empty.height()), (16, 16));
}

#[test]
fn the_same_document_gives_the_same_png() {
    let text = "\
palette p
  t #8b5a2b
  r #c0392b
  w #f4f4f4
  g #a0d8ff80
voxels mushroom 5x5x4 palette=p
  level 0
    .....
    ..t..
    .ttt.
    ..t..
    .....
  level 2
    rrrrr
    rwrrr
    rrrrr
    rrrwr
    rrrrr
  level 3
    .....
    .rrr.
    .rwr.
    .rrr.
    .....
voxels pane 3x1x2 palette=p origin=0,0,0 unit=0.1
  level 0
    tgt
  level 1
    ttt
";
    let doc = Document::parse(text).unwrap();
    let bytes = png::encode(&doc.preview(&PreviewOptions::default()).unwrap());
    let again = png::encode(
        &Document::parse(text)
            .unwrap()
            .preview(&PreviewOptions::default())
            .unwrap(),
    );
    assert_eq!(bytes, again);
    // A golden hash: the same on every platform.
    let hash = common::fnv(&bytes);
    assert_eq!(hash, 0x7b5e0908d3099d5a, "{hash:x}");
}

/// The frame color behind the views.
const FRAME: Color = Color::rgb(0x2a, 0x2e, 0x38);

#[test]
fn t_junctions_leave_no_cracks_in_the_3_4_view() {
    // Greedy meshing splits this top surface into a long and two short rectangles.
    let text = "palette p\n  r #ff0000\nvoxels m 2x5x1 palette=p origin=0,0,0\n  level 0\n    r.\n    r.\n    rr\n    rr\n    rr\n";
    for scale in 1..=24 {
        let image = preview(text, scale);
        // The 3/4 view starts after three views of 2, 5 and 2 voxels and three gaps.
        let left = 8 + (2 + 5 + 2) * scale + 3 * 8;
        let is_frame = |x: u32, y: u32| image.pixel(x, y) == Some(FRAME);
        for y in TOP + 2..image.height() - 2 {
            for x in left + 2..image.width() - 2 {
                if !is_frame(x, y) {
                    continue;
                }
                // A frame pixel walled in by the model on all four sides is a crack.
                let walled = |dx: i32, dy: i32| {
                    (1..=2).any(|k| {
                        let (px, py) = ((x as i32 + dx * k) as u32, (y as i32 + dy * k) as u32);
                        !is_frame(px, py) && image.pixel(px, py).is_some_and(|c| c.r > c.g)
                    })
                };
                assert!(
                    !(walled(-1, 0) && walled(1, 0) && walled(0, -1) && walled(0, 1)),
                    "a crack at ({x}, {y}) at scale {scale}"
                );
            }
        }
    }
}

#[test]
fn the_grid_darkens_each_boundary_once() {
    let text = "palette p\n  w #ffffff\nvoxels m 2x2x1 palette=p\n  level 0\n    ww\n    ww\n";
    let doc = Document::parse(text).unwrap();
    let image = doc
        .preview(&PreviewOptions {
            scale: 1,
            grid: true,
        })
        .unwrap();
    // At scale 1 every pixel of the 2 × 2 top view is on a boundary: all alike.
    let left = 8 + 2 + 8 + 2 + 8;
    let c = pixel(&image, left, TOP);
    for (x, y) in [(1, 0), (0, 1), (1, 1)] {
        assert_eq!(pixel(&image, left + x, TOP + y), c);
    }
}

#[test]
fn long_thin_faces_render_quickly() {
    // Rods 256 voxels long, every other row on every other level.
    let mut text = String::from("palette p\n  a #808080\nvoxels m 256x32x32 palette=p\n");
    let full = "a".repeat(256);
    let empty = ".".repeat(256);
    for y in (0..32).step_by(2) {
        text.push_str(&format!("  level {y}\n"));
        for z in 0..32 {
            text.push_str(&format!(
                "    {}\n",
                if z % 2 == 0 { &full } else { &empty }
            ));
        }
    }
    let doc = Document::parse(&text).unwrap();
    let start = std::time::Instant::now();
    doc.preview(&PreviewOptions::default()).unwrap();
    assert!(
        start.elapsed() < std::time::Duration::from_secs(60),
        "{:?}",
        start.elapsed()
    );
}
