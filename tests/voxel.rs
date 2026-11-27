use meshbox::{Document, ErrorCode, Limits, check, check_with};

const MUSHROOM: &str = "\
palette forest
  t #8b5a2b
  r #c0392b
  w #f4f4f4

voxels mushroom 5x4x3 palette=forest origin=2,0,2 unit=1/16
  level 0
    .....
    ..t..
    .....
    .....
  level 2
    rrrrr
    rwrrr
    rrrrr
    rrrrw
";

#[test]
fn levels_rows_and_columns_map_to_coordinates() {
    let doc = Document::parse(MUSHROOM).unwrap();
    let m = doc.model("mushroom").unwrap();
    assert_eq!(m.size(), [5, 4, 3]);
    assert_eq!(m.origin(), [2.0, 0.0, 2.0]);
    assert_eq!(m.unit(), 0.0625);
    // Level 0, second row from the top (z = 1, toward the back), third column.
    assert_eq!(m.voxel(2, 0, 1), Some('t'));
    assert_eq!(m.voxel(2, 0, 0), None);
    // A missing level is empty.
    assert!((0..5).all(|x| (0..4).all(|z| m.voxel(x, 1, z).is_none())));
    // Columns run along +X, rows toward +Z (the front, the bottom of the text).
    assert_eq!(m.voxel(1, 2, 1), Some('w'));
    assert_eq!(m.voxel(4, 2, 3), Some('w'));
    assert_eq!(m.voxel(0, 2, 0), Some('r'));
    assert_eq!(m.voxel(5, 2, 0), None);
    assert_eq!(m.solid_count(), 21);
    assert_eq!(doc.palettes()[m.palette_index()].name(), "forest");
}

#[test]
fn defaults() {
    let doc = Document::parse("palette p\n  a #ffffff\nvoxels m 3x4x1 palette=p\n").unwrap();
    let m = doc.model("m").unwrap();
    // The pivot is the center of the bottom face; a voxel is 1/16 m.
    assert_eq!(m.origin(), [1.5, 0.0, 2.0]);
    assert_eq!(m.unit(), 1.0 / 16.0);
    assert_eq!(m.solid_count(), 0);
}

fn codes(text: &str) -> Vec<(usize, usize, ErrorCode)> {
    check(text)
        .iter()
        .map(|e| {
            let p = e.position.unwrap();
            (p.line, p.column, e.code)
        })
        .collect()
}

#[test]
fn model_errors_have_codes_and_positions() {
    let p = "palette p\n  a #ffffff\n";
    // A row too long, a row too short, and too few rows.
    assert_eq!(
        codes(&format!(
            "{p}voxels m 2x2x1 palette=p\n  level 0\n    aaa\n    a\n"
        )),
        [
            (5, 7, ErrorCode::SizeMismatch),
            (6, 6, ErrorCode::SizeMismatch)
        ]
    );
    assert_eq!(
        codes(&format!(
            "{p}voxels m 2x2x1 palette=p\n  level 0\n    aa\n    a\n"
        )),
        [(6, 6, ErrorCode::SizeMismatch)]
    );
    assert_eq!(
        codes(&format!("{p}voxels m 2x2x1 palette=p\n  level 0\n    aa\n")),
        [(4, 3, ErrorCode::SizeMismatch)]
    );
    assert_eq!(
        codes(&format!(
            "{p}voxels m 1x1x1 palette=p\n  level 0\n    a\n    a\n"
        )),
        [(6, 5, ErrorCode::SizeMismatch)]
    );
    // A duplicate level and a level out of range.
    assert_eq!(
        codes(&format!(
            "{p}voxels m 1x1x2 palette=p\n  level 0\n    a\n  level 0\n    a\n  level 2\n    a\n"
        )),
        [
            (6, 9, ErrorCode::DuplicateName),
            (8, 9, ErrorCode::InvalidArgument)
        ]
    );
    // A key not in the palette, an unknown palette, a bad size and bad options.
    assert_eq!(
        codes(&format!(
            "{p}voxels m 3x1x1 palette=p\n  level 0\n    a.b\n"
        )),
        [(5, 7, ErrorCode::UnknownName)]
    );
    assert_eq!(
        codes("voxels m 1x1x1 palette=q\n"),
        [(1, 24, ErrorCode::UnknownName)]
    );
    assert_eq!(
        codes("voxels m 1x1 palette=q\n"),
        [(1, 10, ErrorCode::Syntax)]
    );
    assert_eq!(
        codes("voxels m 1x0x1 palette=q\n"),
        [(1, 12, ErrorCode::InvalidArgument)]
    );
    assert_eq!(codes("voxels m 1x1x1\n"), [(1, 15, ErrorCode::Syntax)]);
    assert_eq!(
        codes(&format!("{p}voxels m 1x1x1 palette=p unit=0\n")),
        [(3, 31, ErrorCode::InvalidArgument)]
    );
    assert_eq!(
        codes(&format!("{p}voxels m 1x1x1 palette=p origin=1,2\n")),
        [(3, 33, ErrorCode::Syntax)]
    );
    assert_eq!(
        codes(&format!("{p}voxels m 1x1x1 palette=p color=red\n")),
        [(3, 26, ErrorCode::Syntax)]
    );
    assert_eq!(
        codes(&format!("{p}voxels m 1x1x1 palette=p\n  layer 0\n")),
        [(4, 3, ErrorCode::Syntax)]
    );
    assert_eq!(
        codes(&format!(
            "{p}voxels m 1x1x1 palette=p\nvoxels m 1x1x1 palette=p\n"
        )),
        [(4, 8, ErrorCode::DuplicateName)]
    );
}

#[test]
fn sizes_past_the_limits_are_refused_before_allocation() {
    let p = "palette p\n  a #ffffff\n";
    // A side past the limit.
    assert_eq!(
        codes(&format!("{p}voxels m 257x1x1 palette=p\n")),
        [(3, 10, ErrorCode::LimitExceeded)]
    );
    // Sides within the limit whose product is not: refused without allocating
    // 256³ cells per model.
    let limits = Limits {
        max_voxels: 1000,
        ..Limits::default()
    };
    let errors = check_with(
        &format!("{p}voxels m 10x10x10 palette=p\nvoxels n 1x1x1 palette=p\n"),
        &limits,
    );
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::LimitExceeded);
    assert_eq!(errors[0].position.unwrap().line, 4);
    // Huge sides are a limit error, not an overflow.
    let text = format!("{p}voxels m 99999999999999999999x1x1 palette=p\n");
    assert_eq!(codes(&text), [(3, 10, ErrorCode::LimitExceeded)]);
}

#[test]
fn every_row_of_a_level_is_checked() {
    let p = "palette p\n  a #ffffff\n";
    assert_eq!(
        codes(&format!(
            "{p}voxels m 2x2x1 palette=p\n  level 0\n    a\n    az\n"
        )),
        [
            (5, 6, ErrorCode::SizeMismatch),
            (6, 6, ErrorCode::UnknownName)
        ]
    );
}

#[test]
fn huge_raised_sides_do_not_overflow() {
    let limits = Limits {
        max_voxel_side: u32::MAX,
        ..Limits::default()
    };
    let text = "palette p\n  a #ffffff\nvoxels m 4194304x2097152x2097152 palette=p\n";
    let errors = check_with(text, &limits);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::LimitExceeded);
}

#[test]
fn palettes_too_large_for_a_model_are_refused() {
    let limits = Limits {
        max_palette_colors: 70_000,
        ..Limits::default()
    };
    let mut text = String::from("palette p\n");
    for i in 0..65_536u32 {
        let c = char::from_u32(0x1_0000 + i).unwrap();
        text.push_str(&format!("  {c} #ffffff\n"));
    }
    text.push_str("voxels m 1x1x1 palette=p\n");
    let errors = check_with(&text, &limits);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::LimitExceeded);
}

#[test]
fn extra_rows_still_have_their_keys_checked() {
    let p = "palette p\n  a #ffffff\n";
    assert_eq!(
        codes(&format!(
            "{p}voxels m 1x1x1 palette=p\n  level 0\n    a\n    z\n"
        )),
        [
            (6, 5, ErrorCode::SizeMismatch),
            (6, 5, ErrorCode::UnknownName)
        ]
    );
}

#[test]
fn raised_limits_cannot_overflow_the_allocation() {
    let limits = Limits {
        max_voxel_side: u32::MAX,
        max_voxels: u64::MAX,
        ..Limits::default()
    };
    let text = "palette p\n  a #ffffff\nvoxels m 4194304x2097152x2097152 palette=p\n";
    let errors = check_with(text, &limits);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::LimitExceeded);
}

#[test]
fn rows_of_the_wrong_width_still_have_their_keys_checked() {
    let p = "palette p\n  a #ffffff\n";
    assert_eq!(
        codes(&format!(
            "{p}voxels m 1x1x1 palette=p\n  level 0\n    a\n    az\n"
        )),
        [
            (6, 5, ErrorCode::SizeMismatch),
            (6, 6, ErrorCode::SizeMismatch),
            (6, 6, ErrorCode::UnknownName)
        ]
    );
}

#[test]
fn diagnostics_are_capped() {
    let mut text = String::from("palette p\n  a #ffffff\nvoxels m 256x1x1 palette=p\n  level 0\n");
    for _ in 0..20 {
        text.push_str("    ");
        text.push_str(&"z".repeat(256));
        text.push('\n');
    }
    let errors = check(&text);
    assert_eq!(errors.len(), 1000);
    assert!(errors.windows(2).all(|w| w[0].position <= w[1].position));
}

#[test]
fn tall_models_index_without_overflow() {
    // Offsets are computed in usize, so a tall model with raised limits cannot wrap
    // them; out-of-range coordinates are None.
    let limits = Limits {
        max_voxel_side: u32::MAX,
        max_voxels: u64::MAX,
        ..Limits::default()
    };
    let text = "palette p\n  a #ffffff\nvoxels m 2x1x3 palette=p\n  level 2\n    .a\n";
    let doc = meshbox::Document::parse_with(text, &limits).unwrap();
    let m = doc.model("m").unwrap();
    assert_eq!(m.voxel(1, 2, 0), Some('a'));
    assert_eq!(m.voxel(0, u32::MAX, 0), None);
}
