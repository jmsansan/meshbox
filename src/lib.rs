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
mod image;
#[allow(dead_code)]
mod json;
mod limits;
mod oklab;
mod palette;
pub mod png;
mod text;
mod voxel;

pub use document::{Document, check, check_with};
pub use error::{Error, ErrorCode, Position, Result};
pub use image::{Color, Image};
pub use limits::Limits;
pub use palette::{Palette, PaletteTexture};
pub use voxel::VoxelModel;
