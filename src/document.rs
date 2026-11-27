use crate::mesh::Mesh;
use crate::palette::{Palette, PaletteTexture};
use crate::voxel::VoxelModel;
use crate::{Error, ErrorCode, Limits, Result, gltf, mesher, png, text};

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

    /// The mesh of the model named `name`, within the default [`Limits`].
    pub fn mesh(&self, name: &str) -> Result<Mesh> {
        self.mesh_with(name, &Limits::default())
    }

    /// The mesh of the model named `name`, within `limits`.
    pub fn mesh_with(&self, name: &str, limits: &Limits) -> Result<Mesh> {
        let model = self.find(name)?;
        let texture = self.palette_texture_with(limits)?;
        mesher::mesh(model, &self.palettes, texture.as_ref(), limits, &mut 0)
    }

    /// The model named `name` as binary glTF, with the palette texture embedded,
    /// within the default [`Limits`].
    pub fn glb(&self, name: &str) -> Result<Vec<u8>> {
        self.glb_with(name, &Limits::default())
    }

    /// The model named `name` as binary glTF, within `limits`.
    pub fn glb_with(&self, name: &str, limits: &Limits) -> Result<Vec<u8>> {
        let model = self.find(name)?;
        let texture = self.palette_texture_with(limits)?;
        let mesh = mesher::mesh(model, &self.palettes, texture.as_ref(), limits, &mut 0)?;
        let png = texture.map(|t| png::encode(t.image()));
        gltf::glb(
            &[gltf::Part {
                name: &model.name,
                mesh: &mesh,
            }],
            png.as_deref(),
        )
    }

    /// Every model meshed, with the palette texture encoded once, within the default
    /// [`Limits`]: what [`Export::glb_files`] writes from. The vertex limit counts all
    /// the models together.
    pub fn export(&self) -> Result<Export> {
        self.export_with(&Limits::default())
    }

    /// Every model meshed, within `limits`.
    pub fn export_with(&self, limits: &Limits) -> Result<Export> {
        let texture = self.palette_texture_with(limits)?;
        let png = texture.as_ref().map(|t| png::encode(t.image()));
        let mut vertices = 0;
        let mut models = Vec::with_capacity(self.models.len());
        for model in &self.models {
            let mesh = mesher::mesh(
                model,
                &self.palettes,
                texture.as_ref(),
                limits,
                &mut vertices,
            )?;
            models.push((model.name.clone(), mesh));
        }
        Ok(Export { models, png })
    }

    fn find(&self, name: &str) -> Result<&VoxelModel> {
        self.model(name)
            .ok_or_else(|| Error::new(ErrorCode::UnknownName, format!("Unknown model `{name}`")))
    }
}

/// Parses `text` and returns every error, ordered by position, up to 1000: an empty
/// list when the text is valid.
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

/// A document's models meshed and its palette texture encoded, ready to write.
///
/// ```
/// let document = meshbox::Document::parse(
///     "palette p\n  r #ff0000\nvoxels dot 1x1x1 palette=p\n  level 0\n    r\n",
/// )?;
/// let export = document.export()?;
/// for file in export.glb_files() {
///     let (name, bytes) = file?;
///     assert_eq!(name, "model-dot.glb");
///     assert_eq!(&bytes[..4], b"glTF");
/// }
/// # Ok::<(), meshbox::Error>(())
/// ```
#[derive(Clone, Debug)]
pub struct Export {
    models: Vec<(String, Mesh)>,
    png: Option<Vec<u8>>,
}

impl Export {
    /// Each model's name and mesh, in definition order.
    pub fn meshes(&self) -> impl Iterator<Item = (&str, &Mesh)> {
        self.models.iter().map(|(n, m)| (n.as_str(), m))
    }

    /// The palette texture as PNG, if the document has one.
    pub fn palette_png(&self) -> Option<&[u8]> {
        self.png.as_deref()
    }

    /// Each model as binary glTF, `model-NAME.glb`, in definition order. Each file is
    /// built when the iterator reaches it, so that writing them one by one keeps one
    /// in memory at a time.
    pub fn glb_files(&self) -> impl Iterator<Item = Result<(String, Vec<u8>)>> + '_ {
        self.models.iter().map(|(name, mesh)| {
            let bytes = gltf::glb(&[gltf::Part { name, mesh }], self.png.as_deref())?;
            Ok((format!("model-{name}.glb"), bytes))
        })
    }
}
