use crate::{Error, Result};

/// Bounds on input and output, checked while text is parsed, before memory for a
/// model or an image is allocated, and while meshes are built.
///
/// The defaults suit hand-written files and untrusted input. Raise them for trusted
/// input that needs more:
///
/// ```
/// use meshbox::Limits;
///
/// let limits = Limits { max_voxel_side: 512, ..Limits::default() };
/// assert_eq!(limits.max_definitions, Limits::default().max_definitions);
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Length of the text in bytes, imported files included. Default 4 MiB.
    pub max_text_bytes: usize,
    /// Number of top-level definitions (palettes, models and so on) in the text with
    /// its imports, and of top-level lines (definitions and imports) in any one
    /// file. Default 4096.
    pub max_definitions: usize,
    /// Keys in one palette, inherited keys included. Default 256.
    pub max_palette_colors: usize,
    /// Width, depth or height of one voxel model in voxels. Default 256.
    pub max_voxel_side: u32,
    /// Voxels of all the models of a document together, empty ones included.
    /// Default 16 Mi.
    pub max_voxels: u64,
    /// Triangles in the mesh of one model. Default 1 Mi.
    pub max_triangles: u64,
    /// Vertices in the meshes of all the models of a document together. Default 4 Mi.
    pub max_vertices: u64,
    /// Width or height of any image the crate creates, previews included.
    /// Default 8192.
    pub max_image_side: u32,
    /// Pixels of any image the crate creates. Default 16 Mi (64 MiB of RGBA8).
    pub max_image_pixels: u64,
    /// Depth of definitions built on other definitions, such as palettes copied
    /// `from=` another, and of imports. Default 32.
    pub max_nesting: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_text_bytes: 4 * 1024 * 1024,
            max_definitions: 4096,
            max_palette_colors: 256,
            max_voxel_side: 256,
            max_voxels: 16 * 1024 * 1024,
            max_triangles: 1024 * 1024,
            max_vertices: 4 * 1024 * 1024,
            max_image_side: 8192,
            max_image_pixels: 16 * 1024 * 1024,
            max_nesting: 32,
        }
    }
}

impl Limits {
    /// Checks that an image of `width` × `height` pixels is within the image limits.
    pub fn check_image(&self, width: u32, height: u32) -> Result<()> {
        if width as u64 * height as u64 > isize::MAX as u64 / 4 {
            return Err(Error::limit(
                "Image bytes exceed the platform allocation limit",
            ));
        }
        if width > self.max_image_side || height > self.max_image_side {
            return Err(Error::limit(format!(
                "Image of {width}x{height} pixels is larger than the limit of {} pixels a side",
                self.max_image_side
            )));
        }
        if width as u64 * height as u64 > self.max_image_pixels {
            return Err(Error::limit(format!(
                "Image of {width}x{height} pixels has more than the limit of {} pixels",
                self.max_image_pixels
            )));
        }
        Ok(())
    }
}
