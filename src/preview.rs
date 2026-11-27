//! Preview sheets: each model from the front, the right, the top and a 3/4 view, so
//! that an author, or an assistant, can look at what the text makes and correct it.

use crate::mesh::Mesh;
use crate::palette::Palette;
use crate::raster::{Canvas, Mode, SUBPIXEL, Vertex, blend};
use crate::voxel::VoxelModel;
use crate::{Color, Error, ErrorCode, Image, Limits, Result, label_font};

/// How a preview sheet is drawn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewOptions {
    /// Pixels per voxel in the front, right and top views, 1 to 32 (default 8). The
    /// 3/4 view is drawn a quarter larger, so that its voxels are as big.
    pub scale: u32,
    /// Thin lines at voxel boundaries on the top view, so that coordinates can be
    /// counted (default on).
    pub grid: bool,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        Self {
            scale: 8,
            grid: true,
        }
    }
}

const BACKGROUND: Color = Color::rgb(0x16, 0x18, 0x1d);
const FRAME: Color = Color::rgb(0x2a, 0x2e, 0x38);
const LABEL: Color = Color::rgb(0xd8, 0xdc, 0xe4);
const GRID: Color = Color::rgba(0, 0, 0, 80);
const MARGIN: u32 = 8;
const GAP: u32 = 8;
const ROW_GAP: u32 = 12;
const LABEL_GAP: u32 = 4;

/// The four views, in the order drawn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum View {
    /// From +Z looking toward -Z: X right, Y up.
    Front,
    /// From +X looking toward -X: -Z right, Y up.
    Right,
    /// From +Y looking down: X right, +Z down, as the text's levels are written.
    Top,
    /// From the front, right and above, as an isometric camera sees it.
    Iso,
}

const VIEWS: [View; 4] = [View::Front, View::Right, View::Top, View::Iso];

/// The isometric camera's right, up and toward-the-viewer directions, fixed
/// constants so that no platform math function is involved.
const ISO_RIGHT: [f32; 3] = [0.707_106_77, 0.0, -0.707_106_77];
const ISO_UP: [f32; 3] = [-0.408_248_28, 0.816_496_6, -0.408_248_28];
const ISO_TOWARD: [f32; 3] = [0.577_350_26, 0.577_350_26, 0.577_350_26];
/// The light's direction, toward the light: from above, the front and the right, so
/// that the top is brightest and the front and right sides differ.
const LIGHT: [f32; 3] = [0.399_501, 0.799_002, 0.449_439];
const AMBIENT: f32 = 0.45;
const DIFFUSE: f32 = 0.55;

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl View {
    /// `(x right, y down, depth toward the viewer)` of a point, in meters.
    fn project(self, p: [f32; 3]) -> [f32; 3] {
        match self {
            View::Front => [p[0], -p[1], p[2]],
            View::Right => [-p[2], -p[1], p[0]],
            View::Top => [p[0], p[2], p[1]],
            View::Iso => [dot(p, ISO_RIGHT), -dot(p, ISO_UP), dot(p, ISO_TOWARD)],
        }
    }
    fn toward(self) -> [f32; 3] {
        match self {
            View::Front => [0.0, 0.0, 1.0],
            View::Right => [1.0, 0.0, 0.0],
            View::Top => [0.0, 1.0, 0.0],
            View::Iso => ISO_TOWARD,
        }
    }
}

/// One view laid out: its size, and how meters map to its pixels.
struct Frame {
    view: View,
    width: u32,
    height: u32,
    /// The projected box's smallest x and y, in meters.
    min: [f32; 2],
    /// Pixels per meter.
    ppm: f32,
}

impl Frame {
    fn new(view: View, model: &VoxelModel, scale: u32) -> Frame {
        let [w, d, h] = model.size;
        let corner = |x: u32, y: u32, z: u32| {
            crate::mesher::position([x as i32, y as i32, z as i32], model.origin, model.unit)
        };
        let corners = [
            corner(0, 0, 0),
            corner(w, 0, 0),
            corner(0, h, 0),
            corner(w, h, 0),
            corner(0, 0, d),
            corner(w, 0, d),
            corner(0, h, d),
            corner(w, h, d),
        ];
        let projected = corners.map(|c| view.project(c));
        let fold = |i: usize, f: fn(f32, f32) -> f32| {
            projected.iter().map(|p| p[i]).fold(projected[0][i], f)
        };
        let (min_x, max_x) = (fold(0, f32::min), fold(0, f32::max));
        let (min_y, max_y) = (fold(1, f32::min), fold(1, f32::max));
        let pixels_per_voxel = match view {
            View::Iso => scale as f32 * 1.25,
            _ => scale as f32,
        };
        let ppm = pixels_per_voxel / model.unit;
        // A hair less than the exact size, so that rounding in meters does not add a
        // pixel to the axis views, whose sizes are whole voxels.
        let side = |span: f32| ((span * ppm - 0.001).ceil() as u32).max(1);
        Frame {
            view,
            width: side(max_x - min_x),
            height: side(max_y - min_y),
            min: [min_x, min_y],
            ppm,
        }
    }

    fn vertex(&self, p: [f32; 3]) -> Vertex {
        let s = self.view.project(p);
        let fixed = |v: f32| (v * self.ppm * SUBPIXEL).round() as i64;
        [
            fixed(s[0] - self.min[0]),
            fixed(s[1] - self.min[1]),
            fixed(s[2]),
        ]
    }
}

/// The color of a face with `normal`, lit.
fn shade(color: Color, normal: [f32; 3]) -> Color {
    let light = AMBIENT + DIFFUSE * dot(normal, LIGHT).max(0.0);
    let factor = (light * 256.0).round() as u32;
    let lit = |c: u8| ((c as u32 * factor + 128) >> 8).min(255) as u8;
    Color::rgba(lit(color.r), lit(color.g), lit(color.b), color.a)
}

/// Draws one view of `mesh` into a new image.
fn render(frame: &Frame, mesh: &Mesh, palette: &Palette, limits: &Limits) -> Result<Image> {
    let mut image = Image::new_with(frame.width, frame.height, limits)?;
    image.fill_rect(0, 0, frame.width, frame.height, FRAME);
    let mut canvas = Canvas::new(image);
    let toward = frame.view.toward();
    let triangle = |t: &[u32]| -> Option<([Vertex; 3], Color)> {
        let first = t[0] as usize;
        let normal = mesh.normals[first];
        // Faces turned away from the camera are hidden by those that face it.
        if dot(normal, toward) <= 0.0 {
            return None;
        }
        let v = [0, 1, 2].map(|k| frame.vertex(mesh.positions[t[k] as usize]));
        let key = mesh.quad_keys[first / 4] as usize;
        let color = palette
            .keys
            .get(key.wrapping_sub(1))
            .map_or(Color::TRANSPARENT, |&(_, c)| c);
        Some((v, shade(color, normal)))
    };
    for t in mesh.opaque_indices().chunks_exact(3) {
        if let Some((v, color)) = triangle(t) {
            canvas.triangle(v, color, Mode::Opaque);
        }
    }
    // Translucent faces after the opaque ones, the farthest first.
    let mut clear: Vec<([Vertex; 3], Color)> = mesh
        .translucent_indices()
        .chunks_exact(3)
        .filter_map(triangle)
        .collect();
    clear.sort_by_key(|(v, _)| v[0][2] + v[1][2] + v[2][2]);
    for (v, color) in clear {
        canvas.triangle(v, color, Mode::Blend);
    }
    Ok(canvas.image)
}

/// The preview sheet of `models`: a row per model, its name above four views.
pub(crate) fn sheet(
    models: &[(&VoxelModel, Mesh)],
    palettes: &[Palette],
    options: &PreviewOptions,
    limits: &Limits,
) -> Result<Image> {
    if !(1..=32).contains(&options.scale) {
        return Err(Error::new(
            ErrorCode::InvalidArgument,
            format!("A preview's scale is 1 to 32, not {}", options.scale),
        ));
    }
    let mut rows = Vec::new();
    let (mut width, mut height) = (MARGIN, MARGIN);
    for (model, mesh) in models {
        let frames = VIEWS.map(|v| Frame::new(v, model, options.scale));
        let label = format!(
            "{} {}x{}x{}",
            model.name, model.size[0], model.size[1], model.size[2]
        );
        let row_width = frames.iter().map(|f| f.width).sum::<u32>() + GAP * 3;
        let row_width = row_width.max(label_font::width(&label));
        let row_height =
            label_font::HEIGHT + LABEL_GAP + frames.iter().map(|f| f.height).max().unwrap_or(1);
        // Each view is within the image limits before anything is drawn.
        for f in &frames {
            limits.check_image(f.width, f.height)?;
        }
        width = width.max(MARGIN + row_width.min(u32::MAX - 2 * MARGIN) + MARGIN);
        height = height.saturating_add(row_height + ROW_GAP);
        rows.push((model, mesh, frames, label, row_height));
    }
    let height = if rows.is_empty() {
        2 * MARGIN
    } else {
        height - ROW_GAP + MARGIN
    };
    let width = width.max(2 * MARGIN);
    let mut sheet = Image::new_with(width, height, limits)?;
    sheet.fill_rect(0, 0, width, height, BACKGROUND);
    let mut y = MARGIN;
    for (model, mesh, frames, label, row_height) in rows {
        label_font::draw(&mut sheet, MARGIN, y, &label, LABEL);
        let palette = &palettes[model.palette];
        let mut x = MARGIN;
        let top = y + label_font::HEIGHT + LABEL_GAP;
        for frame in &frames {
            let mut view = render(frame, mesh, palette, limits)?;
            if frame.view == View::Top && options.grid {
                grid(&mut view, model, options.scale);
            }
            copy(&mut sheet, &view, x, top);
            x += frame.width + GAP;
        }
        y += row_height + ROW_GAP;
    }
    Ok(sheet)
}

/// Darkens the pixels on voxel boundaries of a top view.
fn grid(view: &mut Image, model: &VoxelModel, scale: u32) {
    let [w, d, _] = model.size;
    let line = |view: &mut Image, x: u32, y: u32| {
        if let Some(under) = view.pixel(x, y) {
            view.set_pixel(x, y, blend(under, GRID));
        }
    };
    for i in 0..=w {
        let x = (i * scale).min(view.width() - 1);
        for y in 0..view.height() {
            line(view, x, y);
        }
    }
    for j in 0..=d {
        let y = (j * scale).min(view.height() - 1);
        for x in 0..view.width() {
            // Crossings are darkened once.
            if x % scale != 0 && x != view.width() - 1 {
                line(view, x, y);
            }
        }
    }
}

fn copy(sheet: &mut Image, view: &Image, x: u32, y: u32) {
    for vy in 0..view.height() {
        for vx in 0..view.width() {
            if let Some(c) = view.pixel(vx, vy) {
                sheet.set_pixel(x + vx, y + vy, c);
            }
        }
    }
}
