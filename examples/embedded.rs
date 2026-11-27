//! Builds binary glTF from text compiled into the program: no file system is needed
//! to parse or mesh, so this also builds for `wasm32-unknown-unknown`.
//!
//! ```text
//! cargo run --example embedded            # prints the size of each model's glb
//! cargo run --example embedded -- out     # also writes them to the directory `out`
//! ```

/// The models, compiled into the program from a `.meshbox` file next to it.
const TEXT: &str = include_str!("embedded.meshbox");

fn main() -> Result<(), meshbox::Error> {
    let document = meshbox::Document::parse(TEXT)?;
    let files = document
        .export()?
        .glb_files()
        .collect::<Result<Vec<_>, _>>()?;
    let out = std::env::args().nth(1);
    for (name, bytes) in &files {
        println!("{name}: {} bytes", bytes.len());
        if let Some(dir) = &out {
            std::fs::create_dir_all(dir)?;
            std::fs::write(std::path::Path::new(dir).join(name), bytes)?;
        }
    }
    if let Some(dir) = &out {
        let preview = document.preview(&meshbox::PreviewOptions::default())?;
        meshbox::png::write_file(std::path::Path::new(dir).join("preview.png"), &preview)?;
        println!("preview.png: {}x{}", preview.width(), preview.height());
    }
    Ok(())
}
