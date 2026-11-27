//! Low-poly and voxel 3D models from plain text.
//!
//! meshbox is to 3D what spritebox is to pixel art: models written as text, in safe
//! Rust, deterministic, with exact diagnostics, designed first for AI-assisted
//! authoring. An assistant cannot model with a mouse, but it can write precise text,
//! render it, look at the preview and correct the text.
//!
//! The crate depends on nothing but the standard library.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod document;
mod error;
mod gltf;
mod image;
#[allow(dead_code)]
mod json;
mod label_font;
mod limits;
mod mesh;
mod mesher;
mod oklab;
mod palette;
pub mod png;
mod preview;
mod raster;
mod text;
mod voxel;

pub use document::{
    Document, Export, check, check_file_with_loader, check_with, check_with_loader,
};
pub use error::{Error, ErrorCode, Position, Result};
pub use image::{Color, Image};
pub use limits::Limits;
pub use mesh::{Bounds, Mesh, MeshStats};
pub use palette::{Palette, PaletteTexture};
pub use preview::PreviewOptions;
pub use voxel::VoxelModel;

/// The README's Rust examples, compiled and run as doc tests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeDoctests;
