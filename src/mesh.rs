//! Meshes: plain vertex and index data, ready for a GPU or a file.

/// A triangle mesh: positions, normals and UVs per vertex, and `u32` indices, three
/// per triangle, counter-clockwise seen from the front.
///
/// Positions are in meters, relative to the model's origin. UVs point into the
/// document's [`PaletteTexture`](crate::PaletteTexture). The triangles before
/// [`Mesh::translucent_start`] are opaque; the rest have translucent colors and are
/// drawn with blending, after the opaque ones.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    /// Vertex positions in meters.
    pub positions: Vec<[f32; 3]>,
    /// Unit normals, one per vertex.
    pub normals: Vec<[f32; 3]>,
    /// Texture coordinates, `(0, 0)` at the top left of the texture.
    pub uvs: Vec<[f32; 2]>,
    /// Three indices per triangle.
    pub indices: Vec<u32>,
    /// Where the translucent triangles start in [`Mesh::indices`].
    pub translucent_start: usize,
    pub(crate) naive_triangles: u64,
    /// For each quad (four vertices), its key's index in the palette plus one.
    pub(crate) quad_keys: Vec<u16>,
}

/// The counts of a mesh.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeshStats {
    /// Vertices.
    pub vertices: u64,
    /// Triangles.
    pub triangles: u64,
    /// Triangles that a naive mesher, one quad per visible voxel face, would make.
    pub naive_triangles: u64,
}

/// An axis-aligned box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    /// The smallest x, y and z.
    pub min: [f32; 3],
    /// The largest x, y and z.
    pub max: [f32; 3],
}

impl Mesh {
    /// The counts of vertices and triangles.
    pub fn stats(&self) -> MeshStats {
        MeshStats {
            vertices: self.positions.len() as u64,
            triangles: self.indices.len() as u64 / 3,
            naive_triangles: self.naive_triangles,
        }
    }

    /// The box around every vertex, or `None` for an empty mesh.
    pub fn bounds(&self) -> Option<Bounds> {
        let first = *self.positions.first()?;
        let mut b = Bounds {
            min: first,
            max: first,
        };
        for p in &self.positions {
            for (i, &v) in p.iter().enumerate() {
                b.min[i] = b.min[i].min(v);
                b.max[i] = b.max[i].max(v);
            }
        }
        Some(b)
    }

    /// The opaque triangles' indices.
    pub fn opaque_indices(&self) -> &[u32] {
        &self.indices[..self.translucent_start]
    }

    /// The translucent triangles' indices.
    pub fn translucent_indices(&self) -> &[u32] {
        &self.indices[self.translucent_start..]
    }

    /// Every vertex attribute and index as little-endian bytes, in a fixed order:
    /// what golden tests hash.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for p in &self.positions {
            p.iter().for_each(|v| out.extend(v.to_le_bytes()));
        }
        for n in &self.normals {
            n.iter().for_each(|v| out.extend(v.to_le_bytes()));
        }
        for t in &self.uvs {
            t.iter().for_each(|v| out.extend(v.to_le_bytes()));
        }
        for i in &self.indices {
            out.extend(i.to_le_bytes());
        }
        out.extend((self.translucent_start as u64).to_le_bytes());
        out
    }
}
