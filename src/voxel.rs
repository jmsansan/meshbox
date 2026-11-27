//! Voxel models: stacked grids of palette keys.

/// A voxel model: a box of `width` × `depth` × `height` cells, each empty or a key
/// of the model's palette.
///
/// Coordinates follow glTF: right-handed, +Y up. `x` runs from -X to +X (the text's
/// columns), `z` from the back (-Z, the top row of a level) to the front (+Z), and
/// `y` from the ground (`level 0`) up.
#[derive(Clone, Debug, PartialEq)]
pub struct VoxelModel {
    pub(crate) name: String,
    pub(crate) size: [u32; 3],
    pub(crate) palette: usize,
    pub(crate) origin: [f32; 3],
    pub(crate) unit: f32,
    /// For each cell, `x` fastest, then `z`, then `y`: 0 for empty, else the key's
    /// index in the palette plus one.
    pub(crate) cells: Vec<u16>,
    /// The palette's keys, copied so that a model answers alone.
    pub(crate) keys: Vec<char>,
}

impl VoxelModel {
    /// The model's name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// `[width, depth, height]` in voxels: along X, Z and Y.
    pub fn size(&self) -> [u32; 3] {
        self.size
    }
    /// The model's pivot in voxel units, `[x, y, z]`, from the corner at the back,
    /// left and bottom. By default the center of the bottom face.
    pub fn origin(&self) -> [f32; 3] {
        self.origin
    }
    /// The side of one voxel in meters. By default 1/16.
    pub fn unit(&self) -> f32 {
        self.unit
    }
    /// The index of the model's palette in [`Document::palettes`](crate::Document::palettes).
    pub fn palette_index(&self) -> usize {
        self.palette
    }
    /// The key at `(x, y, z)`, or `None` when the cell is empty or outside the model.
    pub fn voxel(&self, x: u32, y: u32, z: u32) -> Option<char> {
        let cell = self.cell(x, y, z)?;
        (cell != 0).then(|| self.keys[cell as usize - 1])
    }
    /// The number of cells that are not empty.
    pub fn solid_count(&self) -> usize {
        self.cells.iter().filter(|&&c| c != 0).count()
    }

    pub(crate) fn cell(&self, x: u32, y: u32, z: u32) -> Option<u16> {
        let [w, d, h] = self.size;
        if x >= w || y >= h || z >= d {
            return None;
        }
        Some(self.cells[((y * d + z) * w + x) as usize])
    }
}
