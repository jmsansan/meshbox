use meshbox::{ErrorCode, Limits};

/// The README's table of limits names every limit and gives its default.
#[test]
fn readme_lists_every_default() {
    let readme = include_str!("../README.md");
    let d = Limits::default();
    let expected: Vec<(&str, u64)> = vec![
        ("max_text_bytes", d.max_text_bytes as u64),
        ("max_definitions", d.max_definitions as u64),
        ("max_palette_colors", d.max_palette_colors as u64),
        ("max_voxel_side", d.max_voxel_side as u64),
        ("max_voxels", d.max_voxels),
        ("max_triangles", d.max_triangles),
        ("max_vertices", d.max_vertices),
        ("max_image_side", d.max_image_side as u64),
        ("max_image_pixels", d.max_image_pixels),
        ("max_nesting", d.max_nesting as u64),
    ];
    let rows: Vec<(String, u64)> = readme
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let name = cells.get(1)?.strip_prefix("`max_")?.strip_suffix('`')?;
            let value = cells.get(2)?.parse().ok()?;
            Some((format!("max_{name}"), value))
        })
        .collect();
    let rows: Vec<(&str, u64)> = rows.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    assert_eq!(rows, expected);
}

#[test]
fn image_limits() {
    let limits = Limits::default();
    assert!(limits.check_image(8192, 2048).is_ok());
    let err = limits.check_image(8193, 1).unwrap_err();
    assert_eq!(err.code, ErrorCode::LimitExceeded);
    let err = limits.check_image(8192, 8192).unwrap_err();
    assert_eq!(err.code, ErrorCode::LimitExceeded);
}

#[test]
fn codes_have_stable_names() {
    let names: Vec<&str> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(
        names,
        [
            "syntax",
            "invalid_argument",
            "unknown_name",
            "duplicate_name",
            "size_mismatch",
            "cycle",
            "limit_exceeded",
            "encode_error",
            "io"
        ]
    );
}
