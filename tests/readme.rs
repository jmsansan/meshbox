//! The README's examples and the showcase stay true.

use meshbox::{Document, PreviewOptions, check, png};

/// Every ```meshbox block of the README passes `check`.
#[test]
fn readme_meshbox_blocks_are_valid() {
    let readme = include_str!("../README.md");
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in readme.lines() {
        match &mut current {
            None if line.trim() == "```meshbox" => current = Some(String::new()),
            Some(block) if line.trim() == "```" => {
                blocks.push(std::mem::take(block));
                current = None;
            }
            Some(block) => {
                block.push_str(line);
                block.push('\n');
            }
            None => {}
        }
    }
    assert!(
        current.is_none(),
        "a meshbox block of the README is not closed"
    );
    assert!(blocks.len() >= 2, "the README has meshbox examples");
    for block in &blocks {
        assert_eq!(check(block), [], "{block}");
    }
}

/// The committed preview is what `meshbox render examples/showcase.meshbox` writes.
#[test]
fn the_committed_preview_matches_the_showcase() {
    let text = include_str!("../examples/showcase.meshbox");
    let document = Document::parse(text).unwrap();
    let preview = document.preview(&PreviewOptions::default()).unwrap();
    let committed = include_bytes!("../docs/showcase-preview.png");
    // Blessing writes the new preview, which the next build embeds.
    if std::env::var_os("MESHBOX_BLESS").is_some() {
        std::fs::write("docs/showcase-preview.png", png::encode(&preview)).unwrap();
        return;
    }
    assert!(
        png::encode(&preview) == committed,
        "docs/showcase-preview.png is stale: run `MESHBOX_BLESS=1 cargo test --test readme`"
    );
}
