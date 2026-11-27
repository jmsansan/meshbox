//! Palettes and the palette texture that every model of a document shares.

use std::collections::HashMap;

use crate::{Color, Image, Limits, Result};

/// A named set of one-character keys and their colors, in definition order, the
/// keys inherited through `from=` first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Palette {
    pub(crate) name: String,
    pub(crate) keys: Vec<(char, Color)>,
    /// Each ramp's name and keys, dark to light, inherited ramps first.
    pub(crate) ramps: Vec<(String, Vec<char>)>,
}

impl Palette {
    /// The palette's name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The keys and their colors, in definition order.
    pub fn keys(&self) -> &[(char, Color)] {
        &self.keys
    }
    /// The ramps: each one's name and its keys, dark to light, as spritebox reads them.
    pub fn ramps(&self) -> &[(String, Vec<char>)] {
        &self.ramps
    }
    /// The color of `key`, if the palette has it.
    pub fn color(&self, key: char) -> Option<Color> {
        self.keys.iter().find(|(k, _)| *k == key).map(|&(_, c)| c)
    }
}

/// Every palette of a document in one RGBA8 image: one row per palette, in
/// definition order, and one texel per key, in the palette's key order (`.` too,
/// though models leave it empty). The rest of a row repeats its last texel.
///
/// Each key's UV is the center of its texel, so nearest sampling never reads a
/// neighbor. Every model of a document uses this one texture, so a renderer can
/// draw them all with one material.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteTexture {
    image: Image,
    uvs: HashMap<(String, char), [f32; 2]>,
    texels: HashMap<(usize, char), (u32, u32)>,
}

impl PaletteTexture {
    pub(crate) fn new(palettes: &[Palette], limits: &Limits) -> Result<Option<Self>> {
        let width = palettes.iter().map(|p| p.keys.len()).max().unwrap_or(0);
        if width == 0 {
            return Ok(None);
        }
        let (width, height) = (width as u32, palettes.len() as u32);
        let mut image = Image::new_with(width, height, limits)?;
        let mut uvs = HashMap::new();
        let mut texels = HashMap::new();
        for (row, palette) in palettes.iter().enumerate() {
            let mut last = Color::TRANSPARENT;
            let mut column = 0;
            for &(key, color) in &palette.keys {
                image.set_pixel(column, row as u32, color);
                uvs.insert(
                    (palette.name.clone(), key),
                    texel_center(column, row as u32, width, height),
                );
                texels.insert((row, key), (column, row as u32));
                last = color;
                column += 1;
            }
            for x in column..width {
                image.set_pixel(x, row as u32, last);
            }
        }
        Ok(Some(Self { image, uvs, texels }))
    }

    /// The texture.
    pub fn image(&self) -> &Image {
        &self.image
    }

    /// The UV of `key` of palette `palette`: the center of its texel, with `(0, 0)`
    /// at the top left of the image as glTF has it.
    pub fn uv(&self, palette: &str, key: char) -> Option<[f32; 2]> {
        self.uvs.get(&(palette.to_owned(), key)).copied()
    }

    /// The texel of `key` of the palette at `index` in the document.
    #[allow(dead_code)] // For the mesher.
    pub(crate) fn texel(&self, index: usize, key: char) -> Option<(u32, u32)> {
        self.texels.get(&(index, key)).copied()
    }

    /// The UV of the palette at `index`'s `key`.
    #[allow(dead_code)] // For the mesher.
    pub(crate) fn uv_of(&self, index: usize, key: char) -> Option<[f32; 2]> {
        let (x, y) = self.texel(index, key)?;
        Some(texel_center(x, y, self.image.width(), self.image.height()))
    }
}

/// `((x + 0.5) / width, (y + 0.5) / height)`, each a single correctly rounded division.
fn texel_center(x: u32, y: u32, width: u32, height: u32) -> [f32; 2] {
    [
        (2 * x + 1) as f32 / (2 * width) as f32,
        (2 * y + 1) as f32 / (2 * height) as f32,
    ]
}
