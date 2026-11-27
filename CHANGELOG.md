# Changelog

## 0.1.0 (2026-11-27)

The first release: voxel models from text to glTF and a preview sheet.

- Crate skeleton: `Error`, `ErrorCode`, `Position` and `Limits`, a `meshbox` binary,
  and CI for Linux, macOS, Windows, WebAssembly and the minimum Rust version, with a
  check that meshbox has no dependencies.
- The text format: definitions at column 1, blocks indented with spaces, `;`
  comments, `key=value` options, `WxDxH` sizes and `x,y,z` vectors. `Document::parse`
  returns the first error and `check` the errors it finds (a line's first fault, and
  none that only follows from another), up to 1000, ordered by position, each with a
  stable code and, when it has a place, a 1-based line and column.
- `palette` with spritebox's syntax, `from=`, `ramp` lines and `lint=`
  included: a file of palettes is valid in both crates, with names of at most 128
  characters. `Document::palette_texture` puts every palette in one RGBA8 image, one
  row per palette and one texel per key, with a UV per key at the texel's center.
- `voxels NAME WxDxH palette=P [origin=x,y,z] [unit=U]` with `level N` blocks:
  rows from the back (-Z) to the front (+Z), columns along +X, levels up +Y.
- A PNG encoder of the crate's own: indexed (1 to 8 bits, with `tRNS` when a color
  is not opaque) for up to 256 colors and RGBA8 otherwise, with no metadata chunks.
- Greedy meshing: `Document::mesh` gives a `Mesh` of positions, normals, UVs and
  `u32` indices, counter-clockwise, opaque triangles first and translucent ones after
  `translucent_start`. Faces between two opaque voxels, or two voxels of one
  translucent key, are dropped, and faces of one key and direction in a plane are
  merged into rectangles, in a fixed order, so the same text gives the same bytes on
  every platform. `Mesh::stats` and `Mesh::bounds`.
- Binary glTF: `Document::glb`, and `Document::export` with `Export::glb_files`, write
  a file per model with one scene and one node at its origin and, when the model has
  faces, the palette texture embedded as PNG with nearest sampling and clamped edges,
  an opaque material for opaque keys and a blended one for translucent keys.
- `examples/embedded.rs`: glb files from text compiled into the program.
- Preview sheets: `Document::preview` draws each model from the front, the right, the
  top and a 3/4 view, with its name and size above, using a fixed-point CPU
  rasterizer with a depth buffer, flat shading from one light and blended
  translucent faces. The top view has optional grid lines at voxel boundaries, and
  the labels use a built-in 5×7 pixel font. The same document gives the same pixels
  on every platform.
- Imports: `import NAME` (or `import "NAME"`) brings in another file's definitions,
  read through the caller's loader with `Document::parse_with_loader` and
  `check_with_loader`, or `Document::parse_file_with_loader` and
  `check_file_with_loader` for text with a name of its own; nothing touches the file
  system unless the loader does. Names resolve against the importing file's
  directory unless rooted (`/`, a drive or a share), the same on every platform;
  cycles are `cycle`, depth past `max_nesting` (counted where a file is first
  reached) is `limit_exceeded`, and an error in an imported file names it.
  `Document::export_models_with` and `preview_models_with` take a subset of the
  models.
- The `meshbox` command: `check <file> [--json]` and
  `render <file> --out <dir> [--scale <n>] [--model <name>] [--json]`, which writes
  `model-NAME.glb` per model, `palette.png`, `preview.png` and `meshbox.json`, and
  nothing when the file has errors or an output does not fit. `-` reads standard
  input. Exit statuses: 0 success, 1 errors in the file or in what is asked of it, 2
  bad usage, 3 I/O errors.
- The README in full, `examples/showcase.meshbox` (a toadstool, a treasure chest and a
  fence post sharing one palette) and its preview in `docs/showcase-preview.png`; a
  test keeps the README's examples valid and the committed preview current.
