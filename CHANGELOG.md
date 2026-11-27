# Changelog

## Unreleased

- Crate skeleton: `Error`, `ErrorCode`, `Position` and `Limits`, a `meshbox` binary,
  and CI for Linux, macOS, Windows, WebAssembly and the minimum Rust version, with a
  check that meshbox has no dependencies.
- The text format: definitions at column 1, blocks indented with spaces, `;`
  comments, `key=value` options, `WxDxH` sizes and `x,y,z` vectors. `Document::parse`
  returns the first error and `check` returns every error, up to 1000, ordered by
  position, each with a stable code and a 1-based line and column.
- `palette` with exactly spritebox's syntax, `from=`, `ramp` lines and `lint=`
  included: a file of palettes is valid in both crates, with names of at most 128
  characters. `Document::palette_texture` puts every palette in one RGBA8 image, one
  row per palette and one texel per key, with a UV per key at the texel's center.
- `voxels NAME WxDxH palette=P [origin=x,y,z] [unit=U]` with `level N` blocks:
  rows from the back (-Z) to the front (+Z), columns along +X, levels up +Y.
- A PNG encoder of the crate's own: indexed (1 to 8 bits, with `tRNS`) for up to 256
  colors and RGBA8 otherwise, with no metadata chunks.
- Greedy meshing: `Document::mesh` gives a `Mesh` of positions, normals, UVs and
  `u32` indices, counter-clockwise, opaque triangles first and translucent ones after
  `translucent_start`. Faces between solid voxels are dropped and faces of one key and
  direction in a plane are merged into rectangles, in a fixed order, so the same text
  gives the same bytes on every platform. `Mesh::stats` and `Mesh::bounds`.
- Binary glTF: `Document::glb` and `Document::glb_files` write one scene with a node
  per model at its origin, the palette texture embedded as PNG with nearest sampling
  and clamped edges, an opaque material and, for translucent keys, a blended one.
- `examples/embedded.rs`: glb files from text compiled into the program.
- Preview sheets: `Document::preview` draws each model from the front, the right, the
  top and a 3/4 view, with its name and size above, using a fixed-point CPU
  rasterizer with a depth buffer, flat shading from one light and blended
  translucent faces. The top view has optional grid lines at voxel boundaries, and
  the labels use a built-in 5×7 pixel font. The same document gives the same pixels
  on every platform.
