use std::fmt;
use std::str::FromStr;

use crate::{Error, Limits, Result};

/// An sRGB color with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Color {
    /// Red, 0 to 255.
    pub r: u8,
    /// Green, 0 to 255.
    pub g: u8,
    /// Blue, 0 to 255.
    pub b: u8,
    /// Alpha: 0 is fully transparent, 255 opaque.
    pub a: u8,
}

impl Color {
    /// Fully transparent black.
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);
    /// Opaque black.
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Color = Color::rgb(255, 255, 255);

    /// An opaque color.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    /// A color with alpha.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    /// `[r, g, b, a]`.
    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
    /// Whether the color is fully opaque.
    pub const fn is_opaque(self) -> bool {
        self.a == 255
    }
}

impl FromStr for Color {
    type Err = Error;
    /// Parses `#rrggbb`, `#rrggbbaa` or `transparent`.
    fn from_str(text: &str) -> Result<Self> {
        if text == "transparent" {
            return Ok(Color::TRANSPARENT);
        }
        let hex = text
            .strip_prefix('#')
            .filter(|h| (h.len() == 6 || h.len() == 8) && h.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| {
                Error::invalid(format!(
                    "Color `{text}` must be `#rrggbb`, `#rrggbbaa` or `transparent`"
                ))
            })?;
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
        let a = if hex.len() == 8 { byte(6) } else { 255 };
        Ok(Color::rgba(byte(0), byte(2), byte(4), a))
    }
}

impl fmt::Display for Color {
    /// `#rrggbb`, or `#rrggbbaa` when the color is not opaque.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)?;
        if self.a != 255 {
            write!(f, "{:02x}", self.a)?;
        }
        Ok(())
    }
}

/// An RGBA8 image: sRGB-encoded, not premultiplied, tightly packed rows, top row first.
#[derive(Clone, Eq, PartialEq)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

impl Image {
    /// A transparent image. Both sides must be at least 1 and within the default
    /// [`Limits`].
    pub fn new(width: u32, height: u32) -> Result<Self> {
        Self::new_with(width, height, &Limits::default())
    }

    /// A transparent image within `limits`.
    pub fn new_with(width: u32, height: u32, limits: &Limits) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(Error::invalid("An image is at least 1x1 pixels"));
        }
        limits.check_image(width, height)?;
        Ok(Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }
    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }
    /// The RGBA8 bytes, rows top to bottom.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    /// The color at `(x, y)`, or `None` outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<Color> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let p = &self.pixels[i..i + 4];
        Some(Color::rgba(p[0], p[1], p[2], p[3]))
    }
    /// Sets the color at `(x, y)`; outside the image it does nothing.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.pixels[i..i + 4].copy_from_slice(&color.to_array());
    }
    /// Fills the rectangle at `(x, y)` of `w` × `h` pixels, clipped to the image.
    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        let x1 = x.saturating_add(w).min(self.width);
        let y1 = y.saturating_add(h).min(self.height);
        for py in y..y1 {
            for px in x..x1 {
                self.set_pixel(px, py, color);
            }
        }
    }
}
