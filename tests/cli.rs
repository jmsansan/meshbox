mod common;

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use common::json;

const VALID: &str = "\
palette p
  a #c0392b
  g #a0d8ff80
voxels cube 2x2x2 palette=p
  level 0
    aa
    aa
  level 1
    ag
    ..
voxels dot 1x1x1 palette=p
  level 0
    a
";

const INVALID: &str = "palette p\n  a #ff00\nvoxels m 1x1x1 palette=q\n";

/// A fresh directory for one test.
fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("meshbox-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn meshbox(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_meshbox"))
        .args(args)
        .output()
        .unwrap()
}

fn status(output: &Output) -> i32 {
    output.status.code().unwrap()
}

fn listing(dir: &PathBuf) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn exit_statuses() {
    let d = dir("status");
    let valid = d.join("valid.meshbox");
    let invalid = d.join("invalid.meshbox");
    std::fs::write(&valid, VALID).unwrap();
    std::fs::write(&invalid, INVALID).unwrap();
    assert_eq!(status(&meshbox(&["check", valid.to_str().unwrap()])), 0);
    let output = meshbox(&["check", invalid.to_str().unwrap()]);
    assert_eq!(status(&output), 1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(":2:5: error[invalid_argument]: "),
        "{stderr}"
    );
    assert!(stderr.contains(":3:24: error[unknown_name]: "), "{stderr}");
    assert_eq!(status(&meshbox(&["frobnicate", "x"])), 2);
    assert_eq!(status(&meshbox(&["check"])), 2);
    assert_eq!(status(&meshbox(&["render", valid.to_str().unwrap()])), 2);
    assert_eq!(
        status(&meshbox(&["render", "x", "--out", "o", "--scale", "99"])),
        2
    );
    assert_eq!(
        status(&meshbox(&[
            "check",
            d.join("missing.meshbox").to_str().unwrap()
        ])),
        3
    );
    assert_eq!(status(&meshbox(&["--version"])), 0);
    assert_eq!(status(&meshbox(&["--help"])), 0);
}

#[test]
fn json_output() {
    let d = dir("json");
    let valid = d.join("valid.meshbox");
    let invalid = d.join("invalid.meshbox");
    std::fs::write(&valid, VALID).unwrap();
    std::fs::write(&invalid, INVALID).unwrap();
    let output = meshbox(&["check", valid.to_str().unwrap(), "--json"]);
    let doc = json::parse(String::from_utf8(output.stdout).unwrap().trim());
    assert_eq!(doc.get("version").usize(), 1);
    assert_eq!(doc.get("ok"), &json::Json::Bool(true));
    assert_eq!(doc.get("diagnostics").len(), 0);
    assert_eq!(doc.get("written").len(), 0);
    assert_eq!(doc.get("file").str(), valid.to_str().unwrap());

    let output = meshbox(&["check", invalid.to_str().unwrap(), "--json"]);
    assert_eq!(status(&output), 1);
    let doc = json::parse(String::from_utf8(output.stdout).unwrap().trim());
    assert_eq!(doc.get("ok"), &json::Json::Bool(false));
    let first = doc.get("diagnostics").at(0);
    assert_eq!(first.get("code").str(), "invalid_argument");
    assert_eq!(first.get("severity").str(), "error");
    assert_eq!(
        (first.get("line").usize(), first.get("column").usize()),
        (2, 5)
    );
    assert_eq!(
        doc.get("diagnostics").at(1).get("code").str(),
        "unknown_name"
    );
}

#[test]
fn render_writes_exactly_the_listed_files() {
    let d = dir("render");
    let file = d.join("art.meshbox");
    std::fs::write(&file, VALID).unwrap();
    let out = d.join("out");
    let output = meshbox(&[
        "render",
        file.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(
        status(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        listing(&out),
        [
            "meshbox.json",
            "model-cube.glb",
            "model-dot.glb",
            "palette.png",
            "preview.png"
        ]
    );
    let doc = json::parse(String::from_utf8(output.stdout).unwrap().trim());
    assert_eq!(doc.get("written").len(), 5);
    // The metadata names each model's file, size, bounds and counts.
    let meta = json::parse(&std::fs::read_to_string(out.join("meshbox.json")).unwrap());
    assert_eq!(meta.get("format").str(), "meshbox");
    assert_eq!(meta.get("version").usize(), 1);
    let cube = meta.get("models").at(0);
    assert_eq!(cube.get("name").str(), "cube");
    assert_eq!(cube.get("file").str(), "model-cube.glb");
    assert_eq!(cube.get("size").len(), 3);
    assert!(cube.get("triangles").usize() > 0);
    assert!(cube.get("vertices").usize() > 0);
    assert_eq!(cube.get("bounds").get("min").len(), 3);
    // The glb is the library's.
    let doc = meshbox::Document::parse(VALID).unwrap();
    assert_eq!(
        std::fs::read(out.join("model-cube.glb")).unwrap(),
        doc.glb("cube").unwrap()
    );

    // --model writes only that model's glb.
    let one = d.join("one");
    let output = meshbox(&[
        "render",
        file.to_str().unwrap(),
        "--out",
        one.to_str().unwrap(),
        "--model",
        "dot",
    ]);
    assert_eq!(status(&output), 0);
    assert_eq!(
        listing(&one),
        [
            "meshbox.json",
            "model-dot.glb",
            "palette.png",
            "preview.png"
        ]
    );
    let output = meshbox(&[
        "render",
        file.to_str().unwrap(),
        "--out",
        one.to_str().unwrap(),
        "--model",
        "nope",
    ]);
    assert_eq!(status(&output), 1);
}

#[test]
fn render_writes_nothing_when_the_file_has_errors() {
    let d = dir("errors");
    let file = d.join("bad.meshbox");
    std::fs::write(&file, INVALID).unwrap();
    let out = d.join("out");
    let output = meshbox(&[
        "render",
        file.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(status(&output), 1);
    assert!(!out.exists());
}

#[test]
fn standard_input_and_imports() {
    let d = dir("stdin");
    std::fs::write(d.join("palettes.meshbox"), "palette p\n  a #ffffff\n").unwrap();
    std::fs::write(
        d.join("model.meshbox"),
        "import palettes.meshbox\nvoxels m 1x1x1 palette=p\n  level 0\n    a\n",
    )
    .unwrap();
    // Imports are read relative to the file.
    let output = meshbox(&["check", d.join("model.meshbox").to_str().unwrap()]);
    assert_eq!(
        status(&output),
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // An error in an imported file names it.
    std::fs::write(d.join("palettes.meshbox"), "palette p\n  a #fff\n").unwrap();
    let output = meshbox(&["check", d.join("model.meshbox").to_str().unwrap()]);
    assert_eq!(status(&output), 1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("palettes.meshbox:2:5: error[invalid_argument]"),
        "{stderr}"
    );
    // `-` reads standard input.
    let mut child = Command::new(env!("CARGO_BIN_EXE_meshbox"))
        .args(["check", "-", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(VALID.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(status(&output), 0);
    let doc = json::parse(String::from_utf8(output.stdout).unwrap().trim());
    assert_eq!(doc.get("file").str(), "<stdin>");
}

#[test]
fn review_cases() {
    let d = dir("review");
    // A file that imports itself is a cycle, not a duplicate of itself.
    let selfish = d.join("art.meshbox");
    std::fs::write(&selfish, "import art.meshbox\npalette p\n  a #ffffff\n").unwrap();
    let output = meshbox(&["check", selfish.to_str().unwrap(), "--json"]);
    assert_eq!(status(&output), 1);
    let doc = json::parse(String::from_utf8(output.stdout).unwrap().trim());
    assert_eq!(doc.get("diagnostics").len(), 1);
    assert_eq!(doc.get("diagnostics").at(0).get("code").str(), "cycle");
    // Text that is not UTF-8 is an error in the file, with its position.
    let bad = d.join("bad.meshbox");
    std::fs::write(&bad, b"palette p\n  a #ff\xff\n").unwrap();
    let output = meshbox(&["check", bad.to_str().unwrap()]);
    assert_eq!(status(&output), 1);
    assert!(String::from_utf8_lossy(&output.stderr).contains(":2:8: error[syntax]"));
    // A missing import is an I/O error, whatever else follows from it.
    let missing = d.join("missing.meshbox");
    std::fs::write(
        &missing,
        "import nowhere.meshbox\nvoxels m 1x1x1 palette=p\n",
    )
    .unwrap();
    assert_eq!(status(&meshbox(&["check", missing.to_str().unwrap()])), 3);
    // --version does not hide bad usage.
    assert_eq!(status(&meshbox(&["nonsense", "--version"])), 2);
    // An empty document still gets every listed file.
    let empty = d.join("empty.meshbox");
    std::fs::write(&empty, "; nothing yet\n").unwrap();
    let out = d.join("empty-out");
    assert_eq!(
        status(&meshbox(&[
            "render",
            empty.to_str().unwrap(),
            "--out",
            out.to_str().unwrap()
        ])),
        0
    );
    assert_eq!(
        listing(&out),
        ["meshbox.json", "palette.png", "preview.png"]
    );
    // Names that differ only in case would write one file on some systems.
    let cases = d.join("cases.meshbox");
    std::fs::write(
        &cases,
        "palette p\n  a #ffffff\nvoxels Cube 1x1x1 palette=p\nvoxels cube 1x1x1 palette=p\n",
    )
    .unwrap();
    let out = d.join("cases-out");
    assert_eq!(
        status(&meshbox(&[
            "render",
            cases.to_str().unwrap(),
            "--out",
            out.to_str().unwrap()
        ])),
        1
    );
    assert!(!out.exists());
}

#[test]
fn review_cases_two() {
    let d = dir("review-two");
    // The file given takes none of the import depth: a chain as deep as the limit fits.
    let depth = meshbox::Limits::default().max_nesting;
    for i in 1..=depth + 1 {
        std::fs::write(
            d.join(format!("c{i}.meshbox")),
            format!("import c{}.meshbox\n", i + 1),
        )
        .unwrap();
    }
    std::fs::write(d.join(format!("c{}.meshbox", depth)), "").unwrap();
    let root = d.join("c0.meshbox");
    std::fs::write(&root, "import c1.meshbox\n").unwrap();
    assert_eq!(status(&meshbox(&["check", root.to_str().unwrap()])), 0);
    std::fs::write(
        d.join(format!("c{}.meshbox", depth)),
        format!("import c{}.meshbox\n", depth + 1),
    )
    .unwrap();
    std::fs::write(d.join(format!("c{}.meshbox", depth + 1)), "").unwrap();
    assert_eq!(status(&meshbox(&["check", root.to_str().unwrap()])), 1);

    // Nor any of the text: a file exactly at the limit fits.
    let max = meshbox::Limits::default().max_text_bytes;
    let line = format!(";{}\n", "x".repeat(1022));
    let full = d.join("full.meshbox");
    std::fs::write(&full, line.repeat(max / 1024)).unwrap();
    assert_eq!(status(&meshbox(&["check", full.to_str().unwrap()])), 0);
    let over = d.join("over.meshbox");
    std::fs::write(&over, format!("{}\n", line.repeat(max / 1024))).unwrap();
    let output = meshbox(&["check", over.to_str().unwrap(), "--json"]);
    assert_eq!(status(&output), 1);
    assert!(String::from_utf8_lossy(&output.stdout).contains("limit_exceeded"));
    // An import that does not fit in what is left is refused.
    let big = d.join("big.meshbox");
    std::fs::write(&big, "import full.meshbox\n").unwrap();
    let output = meshbox(&["check", big.to_str().unwrap(), "--json"]);
    assert_eq!(status(&output), 1);
    assert!(String::from_utf8_lossy(&output.stdout).contains("limit_exceeded"));

    // Text that is not UTF-8 in an import says where.
    std::fs::write(d.join("bad.meshbox"), b"palette p\n  a #ff\xff\n").unwrap();
    let uses_bad = d.join("uses-bad.meshbox");
    std::fs::write(&uses_bad, "import bad.meshbox\n").unwrap();
    let output = meshbox(&["check", uses_bad.to_str().unwrap()]);
    assert_eq!(status(&output), 1);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("bad.meshbox:2:8: error[syntax]"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // A failed read is status 3 even past the cap of diagnostics.
    let capped = d.join("capped.meshbox");
    std::fs::write(
        &capped,
        format!("{}import nowhere.meshbox\n", "\tx\n".repeat(1000)),
    )
    .unwrap();
    assert_eq!(status(&meshbox(&["check", capped.to_str().unwrap()])), 3);
}

/// One file under two spellings is read once.
#[cfg(windows)]
#[test]
fn a_case_alias_is_not_read_twice() {
    let d = dir("alias");
    let art = d.join("art.meshbox");
    std::fs::write(&art, "import ART.meshbox\npalette p\n  a #ffffff\n").unwrap();
    let output = meshbox(&["check", art.to_str().unwrap(), "--json"]);
    assert_eq!(status(&output), 1);
    let doc = json::parse(String::from_utf8(output.stdout).unwrap().trim());
    assert_eq!(doc.get("diagnostics").len(), 1);
    let shared = d.join("shared.meshbox");
    std::fs::write(&shared, "palette q\n  a #ffffff\n").unwrap();
    let root = d.join("root.meshbox");
    std::fs::write(&root, "import shared.meshbox\nimport SHARED.meshbox\n").unwrap();
    let output = meshbox(&["check", root.to_str().unwrap(), "--json"]);
    let doc = json::parse(String::from_utf8(output.stdout).unwrap().trim());
    assert_eq!(doc.get("diagnostics").len(), 1);
    assert_eq!(
        doc.get("diagnostics").at(0).get("code").str(),
        "duplicate_name"
    );
}

#[test]
fn review_cases_three() {
    let d = dir("review-three");
    // Render does not replace its input.
    let input = d.join("palette.png");
    std::fs::write(&input, "; an empty document\n").unwrap();
    let output = meshbox(&[
        "render",
        input.to_str().unwrap(),
        "--out",
        d.to_str().unwrap(),
    ]);
    assert_eq!(status(&output), 2);
    assert_eq!(
        std::fs::read_to_string(&input).unwrap(),
        "; an empty document\n"
    );
    // An option is not the value of another.
    let scene = d.join("scene.meshbox");
    std::fs::write(&scene, VALID).unwrap();
    let output = meshbox(&["render", scene.to_str().unwrap(), "--out", "--json"]);
    assert_eq!(status(&output), 2);
}

#[test]
fn review_cases_four() {
    let d = dir("review-four");
    // An absolute spelling of the file given is a cycle.
    let a = d.join("a.meshbox");
    let absolute = std::fs::canonicalize(&d).unwrap().join("a.meshbox");
    let spelled = absolute.to_str().unwrap().replace('\\', "/");
    let spelled = spelled.strip_prefix("//?/").unwrap_or(&spelled);
    std::fs::write(&a, format!("import \"{spelled}\"\n")).unwrap();
    let output = meshbox(&["check", a.to_str().unwrap(), "--json"]);
    let doc = json::parse(String::from_utf8(output.stdout).unwrap().trim());
    assert_eq!(doc.get("diagnostics").len(), 1);
    assert_eq!(doc.get("diagnostics").at(0).get("code").str(), "cycle");
}

/// Render does not write through a symbolic link.
#[cfg(unix)]
#[test]
fn render_does_not_follow_links() {
    let d = dir("links");
    let scene = d.join("scene.meshbox");
    std::fs::write(&scene, VALID).unwrap();
    let out = d.join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::os::unix::fs::symlink(out.join("model-cube.glb"), out.join("preview.png")).unwrap();
    let output = meshbox(&[
        "render",
        scene.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(status(&output), 2);
    assert!(!out.join("model-cube.glb").exists());
}

#[test]
fn review_cases_six() {
    let d = dir("review-six");
    // Any model's name may be selected, even one that looks like an option.
    let scene = d.join("scene.meshbox");
    std::fs::write(
        &scene,
        "palette p\n  a #ffffff\nvoxels -h 1x1x1 palette=p\n  level 0\n    a\nvoxels --draft 1x1x1 palette=p\n  level 0\n    a\n",
    )
    .unwrap();
    for name in ["-h", "--draft"] {
        let out = d.join(format!("out{name}"));
        let output = meshbox(&[
            "render",
            scene.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--model",
            name,
        ]);
        assert_eq!(status(&output), 0, "{output:?}");
        assert!(out.join(format!("model-{name}.glb")).exists());
    }
    // A duplicate of a definition in another file names that file.
    std::fs::write(d.join("shared.meshbox"), "palette p\n  a #ffffff\n").unwrap();
    let twice = d.join("twice.meshbox");
    std::fs::write(&twice, "import shared.meshbox\npalette p\n  a #000000\n").unwrap();
    let output = meshbox(&["check", twice.to_str().unwrap()]);
    assert_eq!(status(&output), 1);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("line 1 of `")
            && String::from_utf8_lossy(&output.stderr).contains("/shared.meshbox`"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn review_cases_seven() {
    let d = dir("review-seven");
    // Only the model selected is meshed: another one past the limits does not stop it.
    let mut big = String::from(
        "palette p\n  a #ffffff\nvoxels dot 1x1x1 palette=p\n  level 0\n    a\nvoxels board 64x64x64 palette=p\n",
    );
    for z in 0..64 {
        big.push_str(&format!("  level {z}\n"));
        for y in 0..64 {
            let row: String = (0..64)
                .map(|x| if (x + y + z) % 2 == 0 { 'a' } else { '.' })
                .collect();
            big.push_str(&format!("    {row}\n"));
        }
    }
    let scene = d.join("scene.meshbox");
    std::fs::write(&scene, big).unwrap();
    let out = d.join("out");
    let all = meshbox(&[
        "render",
        scene.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(status(&all), 1);
    let one = meshbox(&[
        "render",
        scene.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--model",
        "dot",
    ]);
    assert_eq!(status(&one), 0, "{one:?}");
    assert!(out.join("model-dot.glb").exists());
}

#[test]
fn review_cases_eight() {
    let d = dir("review-eight");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_meshbox"))
            .args(args)
            .current_dir(&d)
            .output()
            .unwrap()
    };
    // One file reached by a relative and an absolute spelling is one file.
    std::fs::create_dir_all(d.join("props")).unwrap();
    std::fs::write(d.join("shared.meshbox"), "palette p\n  a #ffffff\n").unwrap();
    let full = std::fs::canonicalize(&d).unwrap().join("shared.meshbox");
    let full = full.to_str().unwrap().replace('\\', "/");
    let full = full.strip_prefix("//?/").unwrap_or(&full).to_owned();
    std::fs::write(
        d.join("props/box.meshbox"),
        format!("import \"{full}\"\nvoxels box 1x1x1 palette=p\n  level 0\n    a\n"),
    )
    .unwrap();
    std::fs::write(
        d.join("scene.meshbox"),
        "import shared.meshbox\nimport props/box.meshbox\n",
    )
    .unwrap();
    let output = run(&["check", "scene.meshbox"]);
    assert_eq!(status(&output), 0, "{output:?}");
    // Names in messages are as the author wrote them.
    std::fs::write(
        d.join("twice.meshbox"),
        "import shared.meshbox\npalette p\n  a #000000\n",
    )
    .unwrap();
    let output = run(&["check", "twice.meshbox"]);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("line 1 of `shared.meshbox`"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Bad usage with --json is one JSON object too.
    let output = run(&[
        "render",
        "scene.meshbox",
        "--out",
        "out",
        "--scale",
        "33",
        "--json",
    ]);
    assert_eq!(status(&output), 2);
    let doc = json::parse(String::from_utf8(output.stdout).unwrap().trim());
    assert_eq!(
        doc.get("diagnostics").at(0).get("code").str(),
        "invalid_argument"
    );
}
