mod common;

use meshbox::{Color, Document, ErrorCode, check, png};

fn at(text: &str) -> Vec<(usize, usize, ErrorCode)> {
    check(text)
        .iter()
        .map(|e| {
            let p = e.position.expect("position");
            (p.line, p.column, e.code)
        })
        .collect()
}

#[test]
fn texture_has_one_row_per_palette_and_one_texel_per_key() {
    let doc = Document::parse(
        "palette base\n  .  transparent\n  a  #ff0000\n  b  #00ff00\n  c  #0000ff80\n\npalette night from=base\n  b  #003300\n  d  #ffffff\n",
    )
    .unwrap();
    let night = doc.palette("night").unwrap();
    // Inherited keys first, in the parent's order; replaced keys keep their place.
    let keys: Vec<char> = night.keys().iter().map(|&(k, _)| k).collect();
    assert_eq!(keys, ['.', 'a', 'b', 'c', 'd']);
    assert_eq!(night.color('b'), Some(Color::rgb(0, 0x33, 0)));

    let texture = doc.palette_texture().unwrap().unwrap();
    let image = texture.image();
    assert_eq!((image.width(), image.height()), (5, 2));
    #[rustfmt::skip]
    let expected: Vec<u8> = vec![
        // base: . a b c, then c repeated to the row's end.
        0, 0, 0, 0,  255, 0, 0, 255,  0, 255, 0, 255,  0, 0, 255, 128,  0, 0, 255, 128,
        // night: . a b c d.
        0, 0, 0, 0,  255, 0, 0, 255,  0, 0x33, 0, 255,  0, 0, 255, 128,  255, 255, 255, 255,
    ];
    assert_eq!(image.pixels(), &expected[..]);
    assert_eq!(texture.uv("base", 'a'), Some([0.3, 0.25]));
    assert_eq!(texture.uv("base", 'c'), Some([0.7, 0.25]));
    assert_eq!(texture.uv("night", 'b'), Some([0.5, 0.75]));
    assert_eq!(texture.uv("night", 'd'), Some([0.9, 0.75]));
    // Every key has a texel, `.` too, though models leave it empty.
    assert_eq!(texture.uv("base", '.'), Some([0.1, 0.25]));
    assert_eq!(texture.uv("base", 'd'), None);
    assert_eq!(texture.uv("other", 'a'), None);
    // The texture is a PNG like any other.
    let decoded = common::decode_png(&png::encode(image));
    assert_eq!(decoded.pixels, image.pixels());
}

#[test]
fn no_keys_no_texture() {
    let doc = Document::parse("palette empty\n").unwrap();
    assert_eq!(doc.palette_texture().unwrap(), None);
    let doc = Document::parse("palette dot\n  .  #ff0000\n").unwrap();
    let texture = doc.palette_texture().unwrap().unwrap();
    assert_eq!(texture.uv("dot", '.'), Some([0.5, 0.5]));
    assert_eq!(
        Document::parse("").unwrap().palette_texture().unwrap(),
        None
    );
}

#[test]
fn palette_errors_have_codes_and_positions() {
    // An unknown key in a model, a duplicate key and a bad color.
    let text = "palette p\n  a #ff0000\n  a #00ff00\n  b #12345\n\nvoxels m 2x1x1 palette=p\n  level 0\n    az\n";
    assert_eq!(
        at(text),
        [
            (3, 3, ErrorCode::DuplicateName),
            (4, 5, ErrorCode::InvalidArgument),
            (8, 6, ErrorCode::UnknownName),
        ]
    );
    assert_eq!(at("palette p\n  ab #ff0000\n"), [(2, 3, ErrorCode::Syntax)]);
    assert_eq!(
        at("palette p\n  = #ff0000\n"),
        [(2, 3, ErrorCode::InvalidArgument)]
    );
    assert_eq!(at("palette p\n  a\n"), [(2, 4, ErrorCode::Syntax)]);
    assert_eq!(
        at("palette p from=q\n  a #ff0000\n"),
        [(1, 16, ErrorCode::UnknownName)]
    );
    assert_eq!(
        at("palette p from=q\npalette q from=p\n"),
        [(2, 16, ErrorCode::Cycle)]
    );
    assert_eq!(
        at("palette p\npalette p\n"),
        [(2, 9, ErrorCode::DuplicateName)]
    );
    assert_eq!(at("palette p extra\n"), [(1, 11, ErrorCode::Syntax)]);
}

#[test]
fn spritebox_palettes_are_read_as_spritebox_reads_them() {
    let text = include_str!("fixtures/palettes.spritebox");
    let doc = Document::parse(text).unwrap();
    let field = doc.palette("field").unwrap();
    let hex: Vec<String> = field
        .keys()
        .iter()
        .map(|&(k, c)| format!("{k}{c}"))
        .collect();
    assert_eq!(
        hex,
        [
            ".#00000000",
            "k#1a1c2c",
            "w#f4f4f4",
            "g#38b764",
            "G#a7f070",
            "b#5d275d",
            "p#b13e53",
            "P#ef7d57",
            "o#f4b41b",
            "y#ffe478",
            "c#41a6f6"
        ]
    );
    let dusk = doc.palette("dusk").unwrap();
    assert_eq!(dusk.color('p'), Some(Color::rgb(0x3b, 0x5d, 0xc9)));
    assert_eq!(dusk.color('P'), Some(Color::rgb(0x73, 0xef, 0xf7)));
    assert_eq!(dusk.color('k'), Some(Color::rgb(0x1a, 0x1c, 0x2c)));
    // A ramp made from a base: the colors spritebox makes (its oklab tests pin them).
    let clay = doc.palette("clay").unwrap();
    let made: Vec<[u8; 4]> = clay.keys().iter().map(|&(_, c)| c.to_array()).collect();
    assert_eq!(
        made,
        [
            [119, 64, 58, 255],
            [163, 95, 75, 255],
            [208, 129, 89, 255],
            [236, 174, 128, 255],
            [254, 221, 188, 255]
        ]
    );
}

#[test]
fn spritebox_directives_are_named() {
    let errors = check("palette p\n  a #ff0000\nsprite s 1x1 palette=p\n  a\n");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::Syntax);
    assert!(
        errors[0]
            .message
            .contains("`sprite` is a spritebox directive")
    );
}

#[test]
fn from_depth_does_not_depend_on_definition_order() {
    use meshbox::{Limits, check_with};
    let limits = Limits {
        max_nesting: 1,
        ..Limits::default()
    };
    let forward = "palette a\n  x #ffffff\npalette b from=a\npalette c from=b\n";
    let backward = "palette c from=b\npalette b from=a\npalette a\n  x #ffffff\n";
    let found = |text: &str| -> Vec<(usize, ErrorCode)> {
        check_with(text, &limits)
            .iter()
            .map(|e| (e.position.unwrap().line, e.code))
            .collect()
    };
    // `c` is two steps from `a` either way; `b` is one.
    assert_eq!(found(forward), [(4, ErrorCode::LimitExceeded)]);
    assert_eq!(found(backward), [(1, ErrorCode::LimitExceeded)]);
}

#[test]
fn ramps_are_checked_as_spritebox_checks_them() {
    // A ramp without a base names keys that the palette must have.
    assert_eq!(
        at("palette p\n  ramp shade ab\n"),
        [(2, 14, ErrorCode::UnknownName)]
    );
    assert!(check("palette p\n  a #000000\n  b #ffffff\n  ramp shade ab\n").is_empty());
    // A copied palette keeps its parent's ramps; another ramp may not take their keys,
    // but a ramp of the same name replaces the parent's.
    let parent = "palette p\n  a #000000\n  b #808080\n  c #ffffff\n  ramp shade ab\n";
    assert_eq!(
        at(&format!("{parent}palette q from=p\n  ramp other bc\n")),
        [(7, 14, ErrorCode::DuplicateName)]
    );
    let doc = Document::parse(&format!("{parent}palette q from=p\n  ramp shade bc\n")).unwrap();
    let ramps = doc.palette("q").unwrap().ramps();
    assert_eq!(ramps, [("shade".to_owned(), vec!['b', 'c'])]);
}

#[test]
fn lint_options_are_checked_as_spritebox_checks_them() {
    assert!(check("palette p lint=allow(orphan_pixel, flat_ramp)\n").is_empty());
    assert_eq!(at("palette p lint=bogus\n"), [(1, 16, ErrorCode::Syntax)]);
    assert_eq!(
        at("palette p lint=allow(nope)\n"),
        [(1, 22, ErrorCode::Syntax)]
    );
}

#[test]
fn names_are_short() {
    let long = "n".repeat(129);
    assert_eq!(
        at(&format!("palette {long}\n")),
        [(1, 9, ErrorCode::InvalidArgument)]
    );
    assert!(check(&format!("palette {}\n", "n".repeat(128))).is_empty());
}
