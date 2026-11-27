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

#[test]
fn a_failed_import_is_read_once() {
    let mut reads = 0;
    let mut loader = |name: &str| -> Result<String, Error> {
        reads += 1;
        Err(Error::new(ErrorCode::Io, format!("no file {name}")))
    };
    let text = "import gone.meshbox\n".repeat(50);
    let errors = check_with_loader(&text, &Limits::default(), &mut loader);
    assert_eq!(errors.len(), 1);
    assert_eq!(reads, 1);
}

#[test]
fn definitions_past_the_limit_are_reported() {
    let limits = Limits {
        max_definitions: 3,
        ..Limits::default()
    };
    let mut loader = files(&[("shared.meshbox", "")]);
    let text = format!("{}mesh oops\n", "import shared.meshbox\n".repeat(5));
    let errors = check_with_loader(&text, &limits, &mut loader);
    assert!(
        errors.iter().any(|e| e.code == ErrorCode::LimitExceeded),
        "{errors:?}"
    );
}

#[test]
fn exactly_one_past_the_limit_of_directives_is_reported() {
    let limits = Limits {
        max_definitions: 3,
        ..Limits::default()
    };
    let mut loader = files(&[("shared.meshbox", "")]);
    let at_limit = "import shared.meshbox\n".repeat(3);
    assert!(check_with_loader(&at_limit, &limits, &mut loader).is_empty());
    let past = "import shared.meshbox\n".repeat(4);
    let errors = check_with_loader(&past, &limits, &mut loader);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, ErrorCode::LimitExceeded);
    assert_eq!(errors[0].position.unwrap().line, 4);
}

#[test]
fn a_named_file_is_its_own_import() {
    let mut loader = files(&[("art/b.meshbox", "import a.meshbox\n")]);
    let errors = meshbox::check_file_with_loader(
        "art/a.meshbox",
        "import b.meshbox\npalette p\n  a #ffffff\n",
        &Limits::default(),
        &mut loader,
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, ErrorCode::Cycle);
    assert_eq!(errors[0].file.as_deref(), Some("art/b.meshbox"));
}

#[test]
fn a_loader_error_keeps_its_position() {
    let mut loader = |_: &str| {
        Err(Error::at(
            meshbox::Position::new(2, 5),
            ErrorCode::Syntax,
            "The text is not UTF-8",
        ))
    };
    let errors = check_with_loader("import bad.meshbox\n", &Limits::default(), &mut loader);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].file.as_deref(), Some("bad.meshbox"), "{errors:?}");
    let p = errors[0].position.unwrap();
    assert_eq!((p.line, p.column), (2, 5));
}

#[test]
fn a_root_name_is_resolved_like_an_import() {
    let mut loader = files(&[]);
    let errors = meshbox::check_file_with_loader(
        "./art/a.meshbox",
        "import a.meshbox\n",
        &Limits::default(),
        &mut loader,
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, ErrorCode::Cycle);
}

#[test]
fn an_empty_import_name_is_a_syntax_error() {
    let mut loader = files(&[]);
    let errors = check_with_loader("import \"\"\n", &Limits::default(), &mut loader);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, ErrorCode::Syntax);
}

#[test]
fn a_drive_relative_name_is_refused() {
    let mut loader = files(&[]);
    let errors = check_with_loader("import C:part.meshbox\n", &Limits::default(), &mut loader);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, ErrorCode::InvalidArgument);
}

#[test]
fn review_cases_thirteen() {
    // A byte order mark is not text, in the file parsed or an import.
    let mut loader = files(&[("p.meshbox", "\u{feff}palette field\n  k #000000\n")]);
    let text = format!("\u{feff}import p.meshbox\n{MODEL}");
    assert!(check_with_loader(&text, &Limits::default(), &mut loader).is_empty());
    // One quote is a syntax error, not a file name.
    let errors = check_with_loader("import \"p.meshbox\n", &Limits::default(), &mut loader);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, ErrorCode::Syntax);
}
