use crate::palette::{Palette, PaletteTexture};
use crate::voxel::VoxelModel;
use crate::{Error, Limits, Result, text};

/// A parsed meshbox file: its palettes and its models.
///
/// ```
/// use meshbox::Document;
///
/// let document = Document::parse(
///     "palette p\n  r #ff0000\n\nvoxels dot 1x1x1 palette=p\n  level 0\n    r\n",
/// )?;
/// assert_eq!(document.model("dot").unwrap().voxel(0, 0, 0), Some('r'));
/// # Ok::<(), meshbox::Error>(())
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub(crate) palettes: Vec<Palette>,
    pub(crate) models: Vec<VoxelModel>,
}

impl Document {
    /// Parses `text` within the default [`Limits`], returning the first error.
    pub fn parse(text: &str) -> Result<Self> {
        Self::parse_with(text, &Limits::default())
    }

    /// Parses `text` within `limits`, returning the first error.
    pub fn parse_with(text: &str, limits: &Limits) -> Result<Self> {
        text::parse(text, limits).map_err(|mut errors| errors.swap_remove(0))
    }

    /// The palettes, in definition order.
    pub fn palettes(&self) -> &[Palette] {
        &self.palettes
    }

    /// The palette named `name`.
    pub fn palette(&self, name: &str) -> Option<&Palette> {
        self.palettes.iter().find(|p| p.name == name)
    }

    /// The models, in definition order.
    pub fn models(&self) -> &[VoxelModel] {
        &self.models
    }

    /// The model named `name`.
    pub fn model(&self, name: &str) -> Option<&VoxelModel> {
        self.models.iter().find(|m| m.name == name)
    }

    /// The palette texture that every model shares, within the default [`Limits`];
    /// `None` when no palette has a key with a texel.
    pub fn palette_texture(&self) -> Result<Option<PaletteTexture>> {
        self.palette_texture_with(&Limits::default())
    }

    /// The palette texture, within `limits`.
    pub fn palette_texture_with(&self, limits: &Limits) -> Result<Option<PaletteTexture>> {
        PaletteTexture::new(&self.palettes, limits)
    }
}

/// Parses `text` and returns every error, ordered by position: an empty list when the
/// text is valid.
///
/// ```
/// let errors = meshbox::check("palette p\n  rr #ff0000\nvoxels\n");
/// assert_eq!(errors.len(), 2);
/// assert_eq!(errors[0].position.unwrap().line, 2);
/// ```
pub fn check(text: &str) -> Vec<Error> {
    check_with(text, &Limits::default())
}

/// Parses `text` within `limits` and returns every error, ordered by position.
pub fn check_with(text: &str, limits: &Limits) -> Vec<Error> {
    match text::parse(text, limits) {
        Ok(_) => Vec::new(),
        Err(errors) => errors,
    }
}
