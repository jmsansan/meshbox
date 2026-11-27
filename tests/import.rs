use std::collections::HashMap;

use meshbox::{Document, Error, ErrorCode, Limits, check, check_with_loader};

/// A loader over files held in memory.
fn files(entries: &[(&str, &str)]) -> impl FnMut(&str) -> Result<String, Error> + use<> {
    let map: HashMap<String, String> = entries
        .iter()
        .map(|(n, t)| (n.to_string(), t.to_string()))
        .collect();
    move |name: &str| {
        map.get(name)
            .cloned()
            .ok_or_else(|| Error::new(ErrorCode::Io, format!("no file {name}")))
    }
}

const MODEL: &str = "voxels dot 1x1x1 palette=field\n  level 0\n    k\n";

#[test]
fn a_shared_palette_file_is_imported() {
    let shared = include_str!("fixtures/palettes.spritebox");
    let mut loader = files(&[("art/palettes.spritebox", shared)]);
    let text = format!("import art/palettes.spritebox\n{MODEL}");
    let doc = Document::parse_with_loader(&text, &Limits::default(), &mut loader).unwrap();
    assert!(doc.palette("dusk").is_some());
    assert_eq!(doc.model("dot").unwrap().voxel(0, 0, 0), Some('k'));
    // A quoted name means the same.
    let quoted = format!("import \"art/palettes.spritebox\"\n{MODEL}");
    let again = Document::parse_with_loader(&quoted, &Limits::default(), &mut loader).unwrap();
    assert_eq!(again, doc);
}

#[test]
fn nested_imports_resolve_from_the_importing_file() {
    let mut loader = files(&[
        (
            "art/models.meshbox",
            "import palettes/base.meshbox\nvoxels cube 1x1x1 palette=base\n  level 0\n    a\n",
        ),
        ("art/palettes/base.meshbox", "palette base\n  a #ff0000\n"),
    ]);
    let doc = Document::parse_with_loader(
        "import art/models.meshbox\n",
        &Limits::default(),
        &mut loader,
    )
    .unwrap();
    assert!(doc.model("cube").is_some());
    assert!(doc.palette("base").is_some());
}

#[test]
fn import_errors() {
    let limits = Limits::default();
    // A cycle.
    let mut loader = files(&[
        ("a.meshbox", "import b.meshbox\n"),
        ("b.meshbox", "import a.meshbox\n"),
    ]);
    let errors = check_with_loader("import a.meshbox\n", &limits, &mut loader);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::Cycle);
    assert_eq!(errors[0].file.as_deref(), Some("b.meshbox"));
    // A missing file.
    let errors = check_with_loader("palette p\nimport missing.meshbox\n", &limits, &mut loader);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::Io);
    assert_eq!(errors[0].position.map(|p| (p.line, p.column)), Some((2, 8)));
    assert_eq!(errors[0].file, None);
    // A spritebox sprite in an imported file.
    let mut loader = files(&[(
        "art.spritebox",
        "palette p\n  a #ffffff\nsprite s 1x1 palette=p\n  a\n",
    )]);
    let errors = check_with_loader("import art.spritebox\n", &limits, &mut loader);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::Syntax);
    assert!(errors[0].message.contains("`sprite`"));
    // An error inside an imported file carries the file's name and its own position.
    let mut loader = files(&[("palettes.meshbox", "palette p\n  a #ff00\n")]);
    let errors = check_with_loader("import palettes.meshbox\n", &limits, &mut loader);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::InvalidArgument);
    assert_eq!(errors[0].file.as_deref(), Some("palettes.meshbox"));
    assert_eq!(errors[0].position.map(|p| (p.line, p.column)), Some((2, 5)));
    assert_eq!(
        errors[0].to_string(),
        "palettes.meshbox, line 2, column 5: Color `#ff00` must be `#rrggbb`, `#rrggbbaa` or `transparent`"
    );
}

#[test]
fn imports_need_a_loader() {
    let errors = check("import palettes.meshbox\n");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::InvalidArgument);
    assert!(errors[0].message.contains("parse_with_loader"));
}

#[test]
fn import_depth_is_limited() {
    let names: Vec<String> = (0..40).map(|i| format!("f{i}.meshbox")).collect();
    let texts: Vec<String> = (0..40)
        .map(|i| format!("import f{}.meshbox\n", i + 1))
        .collect();
    let entries: Vec<(&str, &str)> = names
        .iter()
        .zip(&texts)
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
    let mut loader = files(&entries);
    let errors = check_with_loader("import f0.meshbox\n", &Limits::default(), &mut loader);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::LimitExceeded);
}
