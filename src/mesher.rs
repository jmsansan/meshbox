//! Greedy meshing of voxel models.
//!
//! Faces between two solid voxels are dropped; the faces of one key and direction in
//! one plane are merged into rectangles, each two triangles with one flat color. The
//! output depends only on the model: rectangles come in a fixed order (axis, then
//! direction, then plane, then row), and positions are computed with exactly rounded
//! `f32` operations, so the same text gives the same bytes on every platform.
//!
//! Greedy meshing is fast, not optimal: some shapes take a rectangle or two more than
//! the fewest possible. Where a large rectangle meets two small ones, their shared
//! edge has a T-junction; every corner sits exactly on whole voxels, so the surface
//! has no cracks, and each unit length of edge is used once in each direction.

use crate::mesh::Mesh;
use crate::palette::{Palette, PaletteTexture};
use crate::voxel::VoxelModel;
use crate::{Error, ErrorCode, Limits, Result};

/// A rectangle of faces in voxel units: its corners, counter-clockwise seen from
/// outside, its normal axis and sign, and its palette key index (from 1).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Quad {
    pub corners: [[i32; 3]; 4],
    pub axis: usize,
    pub positive: bool,
    pub cell: u16,
}

/// The rectangles of a model's surface, opaque first, and the number of single faces
/// they cover.
/// Stops with `None` once there are more than `max` rectangles, so that a model past
/// the triangle limit costs no more memory than the limit allows.
pub(crate) fn quads(
    model: &VoxelModel,
    palette: &Palette,
    max: u64,
) -> Option<(Vec<Quad>, Vec<Quad>, u64)> {
    let translucent: Vec<bool> = std::iter::once(false)
        .chain(palette.keys.iter().map(|(_, c)| !c.is_opaque()))
        .collect();
    let [w, d, h] = model.size;
    // Sizes along X, Y and Z.
    let dims = [w as i32, h as i32, d as i32];
    // The step in `cells` along X, Y and Z.
    let stride = [1, w as usize * d as usize, w as usize];
    // Whether the face of a voxel of `cell` against a neighbor of `other` shows.
    let visible = |cell: u16, other: u16| -> bool {
        if cell == 0 {
            return false;
        }
        if other == 0 {
            return true;
        }
        match (translucent[cell as usize], translucent[other as usize]) {
            (false, false) => false,
            (false, true) => true,
            (true, false) => false,
            (true, true) => cell != other,
        }
    };
    // Rarely more rectangles than solid voxels; reserving that avoids most growth.
    let solid = model.cells.iter().filter(|&&c| c != 0).count();
    let (mut opaque, mut clear) = (Vec::with_capacity(solid), Vec::new());
    let mut faces = 0u64;
    for axis in 0..3 {
        // The plane's two other axes, so that u × v points along +axis.
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let (nu, nv) = (dims[u] as usize, dims[v] as usize);
        let mut mask = vec![0u16; nu * nv];
        for positive in [false, true] {
            for layer in 0..dims[axis] {
                let mut any = false;
                // The neighbor is outside the model on the last layer of this side.
                let edge = if positive {
                    layer + 1 == dims[axis]
                } else {
                    layer == 0
                };
                for j in 0..nv {
                    let row = layer as usize * stride[axis] + j * stride[v];
                    for i in 0..nu {
                        let index = row + i * stride[u];
                        let cell = model.cells[index];
                        let shown = cell != 0
                            && (edge || {
                                let other = if positive {
                                    model.cells[index + stride[axis]]
                                } else {
                                    model.cells[index - stride[axis]]
                                };
                                visible(cell, other)
                            });
                        mask[j * nu + i] = if shown { cell } else { 0 };
                        if shown {
                            any = true;
                            faces += 1;
                        }
                    }
                }
                if !any {
                    continue;
                }
                let plane = if positive { layer + 1 } else { layer };
                // Greedy: from each unvisited face, grow along u, then along v while
                // the whole row matches.
                for j in 0..nv {
                    let mut i = 0;
                    while i < nu {
                        let cell = mask[j * nu + i];
                        if cell == 0 {
                            i += 1;
                            continue;
                        }
                        let mut width = 1;
                        while i + width < nu && mask[j * nu + i + width] == cell {
                            width += 1;
                        }
                        let mut height = 1;
                        'grow: while j + height < nv {
                            for k in 0..width {
                                if mask[(j + height) * nu + i + k] != cell {
                                    break 'grow;
                                }
                            }
                            height += 1;
                        }
                        for row in j..j + height {
                            mask[row * nu + i..row * nu + i + width].fill(0);
                        }
                        let corner = |du: usize, dv: usize| {
                            let mut c = [0i32; 3];
                            c[axis] = plane;
                            c[u] = (i + du) as i32;
                            c[v] = (j + dv) as i32;
                            c
                        };
                        let mut corners = [
                            corner(0, 0),
                            corner(width, 0),
                            corner(width, height),
                            corner(0, height),
                        ];
                        if !positive {
                            corners = [corners[0], corners[3], corners[2], corners[1]];
                        }
                        let quad = Quad {
                            corners,
                            axis,
                            positive,
                            cell,
                        };
                        if (opaque.len() + clear.len()) as u64 >= max {
                            return None;
                        }
                        if translucent[cell as usize] {
                            clear.push(quad);
                        } else {
                            opaque.push(quad);
                        }
                        i += width;
                    }
                }
            }
        }
    }
    Some((opaque, clear, faces))
}

/// The mesh of `model`, whose palette is `palettes[model.palette]`, with UVs into
/// `texture`. `vertices` counts the vertices of the document's meshes so far.
pub(crate) fn mesh(
    model: &VoxelModel,
    palettes: &[Palette],
    texture: Option<&PaletteTexture>,
    limits: &Limits,
    vertices: &mut u64,
) -> Result<Mesh> {
    let palette = &palettes[model.palette];
    let Some((opaque, clear, faces)) = quads(model, palette, limits.max_triangles / 2) else {
        return Err(Error::new(
            ErrorCode::LimitExceeded,
            format!(
                "Model `{}` meshes to more triangles than the limit of {}",
                model.name, limits.max_triangles
            ),
        ));
    };
    let count = (opaque.len() + clear.len()) as u64;
    *vertices += count * 4;
    if *vertices > limits.max_vertices {
        return Err(Error::new(
            ErrorCode::LimitExceeded,
            format!(
                "With model `{}`, the meshes have more vertices than the limit of {}",
                model.name, limits.max_vertices
            ),
        ));
    }
    let n = count as usize;
    let mut mesh = Mesh {
        positions: Vec::with_capacity(n * 4),
        normals: Vec::with_capacity(n * 4),
        uvs: Vec::with_capacity(n * 4),
        indices: Vec::with_capacity(n * 6),
        translucent_start: opaque.len() * 6,
        naive_triangles: faces * 2,
    };
    // Each key's UV, looked up once.
    let uvs: Vec<[f32; 2]> = model
        .keys
        .iter()
        .map(|&key| {
            texture
                .and_then(|t| t.uv_of(model.palette, key))
                .unwrap_or([0.0, 0.0])
        })
        .collect();
    for quad in opaque.iter().chain(&clear) {
        let uv = uvs[quad.cell as usize - 1];
        let mut normal = [0.0f32; 3];
        normal[quad.axis] = if quad.positive { 1.0 } else { -1.0 };
        let base = mesh.positions.len() as u32;
        for c in quad.corners {
            mesh.positions.push(position(c, model.origin, model.unit));
            mesh.normals.push(normal);
            mesh.uvs.push(uv);
        }
        mesh.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Ok(mesh)
}

/// A corner in voxel units to meters from the origin.
pub(crate) fn position(c: [i32; 3], origin: [f32; 3], unit: f32) -> [f32; 3] {
    [
        (c[0] as f32 - origin[0]) * unit,
        (c[1] as f32 - origin[1]) * unit,
        (c[2] as f32 - origin[2]) * unit,
    ]
}
