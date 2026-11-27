# Changelog

## Unreleased

- Crate skeleton: `Error`, `ErrorCode`, `Position` and `Limits`, a `meshbox` binary,
  and CI for Linux, macOS, Windows, WebAssembly and the minimum Rust version, with a
  check that meshbox has no dependencies.
- The text format: definitions at column 1, blocks indented with spaces, `;`
  comments, `key=value` options, `WxDxH` sizes and `x,y,z` vectors. `Document::parse`
  returns the first error and `check` returns every error, ordered by position, each
  with a stable code and a 1-based line and column.
- `palette` with exactly spritebox's syntax, `from=` and `ramp` lines included: a
  file of palettes is valid in both crates. `Document::palette_texture` puts every
  palette in one RGBA8 image, one row per palette and one texel per key, with a UV
  per key at the texel's center.
- `voxels NAME WxDxH palette=P [origin=x,y,z] [unit=U]` with `level N` blocks:
  rows from the back (-Z) to the front (+Z), columns along +X, levels up +Y.
- A PNG encoder of the crate's own: indexed (1 to 8 bits, with `tRNS`) for up to 256
  colors and RGBA8 otherwise, with no metadata chunks.
