# meshbox

Low-poly and voxel 3D models from plain text, in safe Rust.

meshbox is to 3D what [spritebox](https://github.com/jmsansan/spritebox) is to pixel
art: models are written as text, meshed deterministically, exported as binary glTF and
rendered to a preview sheet. It is designed first for AI-assisted authoring: an
assistant cannot model with a mouse, but it can write precise text, render it, look at
the preview and correct the text.

**meshbox has zero dependencies.** It uses nothing but the Rust standard library: it
brings its own PNG encoder, JSON writer, glTF writer and software rasterizer (as they
land, see the table below). The same text gives the same bytes on every platform,
`wasm32-unknown-unknown` included.

## Capabilities

| Capability | Status |
| --- | --- |
| Text format with exact diagnostics (line, column, stable code) | Available |
| Palettes with spritebox's syntax, shared palette texture | Available |
| Voxel models from stacked grids | Available |
| Greedy meshing to a deterministic mesh | Planned (0.1) |
| Binary glTF (`.glb`) export | Planned (0.1) |
| Four-view preview sheet (PNG) | Planned (0.1) |
| Imports through a loader, in memory and in WebAssembly | Planned (0.1) |
| Command-line tool: `check` and `render` | Planned (0.1) |
| Low-poly primitives, composition and modifiers | Planned (0.2) |
| Heightmap terrain | Planned (0.3) |
| Seeded generators and scattering | Planned (0.4) |
| Rigid animation with pivots and clips | Planned (0.5) |
| Materials and game-ready vertex data | Planned (0.6) |

## Limits

`Limits` bounds the work any input can cause. Every parsing function takes one;
the defaults suit hand-written files and untrusted input.

| Limit | Default | Bounds |
| --- | --- | --- |
| `max_text_bytes` | 4194304 | Bytes of text, imported files included |
| `max_definitions` | 4096 | Top-level definitions |
| `max_palette_colors` | 256 | Keys in one palette |
| `max_voxel_side` | 256 | Width, depth or height of one voxel model |
| `max_voxels` | 16777216 | Voxels of all models together |
| `max_triangles` | 1048576 | Triangles in one model's mesh |
| `max_vertices` | 4194304 | Vertices of all meshes together |
| `max_image_side` | 8192 | Side of any image created |
| `max_image_pixels` | 16777216 | Pixels of any image created |
| `max_nesting` | 32 | Depth of `from=` chains and imports |

## License

MIT
