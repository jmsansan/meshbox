//! Builds binary glTF from text compiled into the program: no file system is needed
//! to parse or mesh, so this also builds for `wasm32-unknown-unknown`.
//!
//! ```text
//! cargo run --example embedded            # prints the size of each model's glb
//! cargo run --example embedded -- out     # also writes them to the directory `out`
//! ```

const TEXT: &str = "\
palette forest
  t #8b5a2b
  T #6b4423
  r #c0392b
  w #f4f4f4
  g #a0d8ff80

; A mushroom: a stem, and a cap with white spots.
voxels mushroom 5x5x4 palette=forest
  level 0
    .....
    ..t..
    .tTt.
    ..t..
    .....
  level 1
    .....
    ..t..
    .tTt.
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

; A pane of glass in a wooden frame.
voxels window 3x1x3 palette=forest origin=1.5,0,0.5
  level 0
    TTT
  level 1
    TgT
  level 2
    TTT
";

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
    Ok(())
}
