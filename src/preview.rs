//! Preview sheets: each model from the front, the right, the top and a 3/4 view, so
//! that an author, or an assistant, can look at what the text makes and correct it.
//!
//! The views are drawn from the model's rectangles in voxel units, projected with
//! integer matrices: every corner lands on an exact subpixel position, so faces that
//! meet, at a T-junction too, leave no crack, and the model's origin and unit do not
//! change a pixel.

use crate::mesher::{Quad, quads};
use crate::palette::Palette;
use crate::raster::{Canvas, SUBPIXEL, Vertex, blend};
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

/// The isometric camera's right and up directions, `(√2/2, 0, -√2/2)` and
/// `(-√6/6, √6/3, -√6/6)`, as constants: no platform math function is involved.
const ISO_RIGHT: f32 = 0.707_106_77;
const ISO_UP_SIDE: f32 = 0.408_248_28;
const ISO_UP: f32 = 0.816_496_6;
/// The light's direction, toward the light: from above, the front and the right, so
/// that the top is brightest and the front and right sides differ.
const LIGHT: [f32; 3] = [0.399_501, 0.799_002, 0.449_439];
const AMBIENT: f32 = 0.45;
const DIFFUSE: f32 = 0.55;
/// Depth steps per voxel: interpolated depths keep 1/65536 of a voxel, so faces a
/// fraction of a voxel apart never tie.
const DEPTH: i64 = 1 << 16;

/// One view laid out: an integer projection of voxel coordinates to subpixels.
struct Frame {
    view: View,
    width: u32,
    height: u32,
    /// Subpixels per voxel along the projection's directions.
    step: [i64; 3],
    /// What is subtracted so that the model's box starts at (0, 0).
    offset: [i64; 2],
}

impl Frame {
    fn new(view: View, model: &VoxelModel, scale: u32) -> Frame {
        let s = scale as i64 * SUBPIXEL;
        // The isometric view is a quarter larger; its steps are rounded once, to
        // whole subpixels, so that every voxel corner projects exactly.
        let iso = |f: f32| (f * scale as f32 * 1.25 * SUBPIXEL as f32).round() as i64;
        let step = match view {
            View::Iso => [iso(ISO_RIGHT), iso(ISO_UP_SIDE), iso(ISO_UP)],
            _ => [s, s, s],
        };
        let mut frame = Frame {
            view,
            width: 1,
            height: 1,
            step,
            offset: [0, 0],
        };
        let [w, d, h] = model.size.map(|v| v as i32);
        let corners = [
            [0, 0, 0],
            [w, 0, 0],
            [0, h, 0],
            [w, h, 0],
            [0, 0, d],
            [w, 0, d],
            [0, h, d],
            [w, h, d],
        ]
        .map(|c| frame.project(c));
        let min = |i: usize| corners.iter().map(|c| c[i]).min().unwrap_or(0);
        let max = |i: usize| corners.iter().map(|c| c[i]).max().unwrap_or(0);
        frame.offset = [min(0), min(1)];
        let side = |span: i64| {
            let pixels = span.div_euclid(SUBPIXEL) + i64::from(span % SUBPIXEL != 0);
            pixels.clamp(1, u32::MAX as i64) as u32
        };
        frame.width = side(max(0) - min(0));
        frame.height = side(max(1) - min(1));
        frame
    }

    /// `(x right, y down, depth toward the viewer)` of a voxel corner, before the
    /// offset: exact integers.
    fn project(&self, c: [i32; 3]) -> Vertex {
        let [x, y, z] = c.map(i64::from);
        let [a, b, e] = self.step;
        match self.view {
            View::Front => [x * a, -y * a, z * DEPTH],
            View::Right => [-z * a, -y * a, x * DEPTH],
            View::Top => [x * a, z * a, y * DEPTH],
            View::Iso => [(x - z) * a, (x + z) * b - y * e, (x + y + z) * DEPTH],
        }
    }

    fn vertex(&self, c: [i32; 3]) -> Vertex {
        let p = self.project(c);
        [p[0] - self.offset[0], p[1] - self.offset[1], p[2]]
    }

    /// Whether a face whose normal is `axis`, `positive` or not, faces the camera.
    fn faces(&self, axis: usize, positive: bool) -> bool {
        match self.view {
            View::Front => axis == 2 && positive,
            View::Right => axis == 0 && positive,
            View::Top => axis == 1 && positive,
            View::Iso => positive,
        }
    }
}

/// The color of a face whose normal is `axis`, `positive` or not, lit.
fn shade(color: Color, axis: usize, positive: bool) -> Color {
    let n = if positive { 1.0 } else { -1.0 };
    let light = AMBIENT + DIFFUSE * (LIGHT[axis] * n).max(0.0);
    let factor = (light * 256.0).round() as u32;
    let lit = |c: u8| ((c as u32 * factor + 128) >> 8).min(255) as u8;
    Color::rgba(lit(color.r), lit(color.g), lit(color.b), color.a)
}

/// Draws one view of a model's rectangles into a new image.
fn render(
    frame: &Frame,
    opaque: &[Quad],
    clear: &[Quad],
    palette: &Palette,
    limits: &Limits,
) -> Result<Image> {
    let mut image = Image::new_with(frame.width, frame.height, limits)?;
    image.fill_rect(0, 0, frame.width, frame.height, FRAME);
    let mut canvas = Canvas::new(image).ok_or_else(|| {
        Error::new(
            ErrorCode::LimitExceeded,
            format!(
                "A view of {}x{} pixels needs a depth buffer larger than this platform can hold",
                frame.width, frame.height
            ),
        )
    })?;
    let color = |q: &Quad| {
        let c = palette
            .keys
            .get(q.cell as usize - 1)
            .map_or(Color::TRANSPARENT, |&(_, c)| c);
        shade(c, q.axis, q.positive)
    };
    let triangles = |q: &Quad| {
        let v = q.corners.map(|c| frame.vertex(c));
        [[v[0], v[1], v[2]], [v[0], v[2], v[3]]]
    };
    for q in opaque.iter().filter(|q| frame.faces(q.axis, q.positive)) {
        for t in triangles(q) {
            canvas.opaque(t, color(q));
        }
    }
    // Translucent faces after the opaque ones, blended at each pixel from the
    // farthest to the nearest.
    // Translucent layers are bounded like the pixels of an image, and never past what
    // the platform can allocate, so that deep stacks of glass cannot exhaust memory.
    let max_fragments = usize::try_from(limits.max_image_pixels)
        .unwrap_or(usize::MAX)
        .min(isize::MAX as usize / (2 * std::mem::size_of::<crate::raster::Fragment>()));
    let mut fragments = Vec::new();
    for q in clear.iter().filter(|q| frame.faces(q.axis, q.positive)) {
        for t in triangles(q) {
            if !canvas.fragments(t, color(q), &mut fragments, max_fragments) {
                return Err(Error::new(
                    ErrorCode::LimitExceeded,
                    format!(
                        "A view has more translucent fragments than the limit of {} pixels",
                        limits.max_image_pixels
                    ),
                ));
            }
        }
    }
    canvas.blend_fragments(fragments);
    Ok(canvas.image)
}

/// The preview sheet of `models`: a row per model, its name above four views.
pub(crate) fn sheet(
    models: &[&VoxelModel],
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
    let (mut width, mut height) = (2 * MARGIN, MARGIN);
    for &model in models {
        let frames = VIEWS.map(|v| Frame::new(v, model, options.scale));
        // Each view is within the image limits before anything is drawn.
        for f in &frames {
            limits.check_image(f.width, f.height)?;
        }
        let label = format!(
            "{} {}x{}x{}",
            model.name, model.size[0], model.size[1], model.size[2]
        );
        let views_width = frames
            .iter()
            .fold(GAP * 3, |sum, f| sum.saturating_add(f.width));
        let row_width = views_width.max(label_font::width(&label));
        let row_height =
            label_font::HEIGHT + LABEL_GAP + frames.iter().map(|f| f.height).max().unwrap_or(1);
        width = width.max(row_width.saturating_add(2 * MARGIN));
        height = height.saturating_add(row_height + ROW_GAP);
        rows.push((model, frames, label, row_height));
    }
    let height = if rows.is_empty() {
        2 * MARGIN
    } else {
        height - ROW_GAP + MARGIN
    };
    let mut sheet = Image::new_with(width, height, limits)?;
    sheet.fill_rect(0, 0, width, height, BACKGROUND);
    let mut y = MARGIN;
    for (model, frames, label, row_height) in rows {
        label_font::draw(&mut sheet, MARGIN, y, &label, LABEL);
        let palette = &palettes[model.palette];
        let (opaque, clear, _) =
            quads(model, palette, limits.max_triangles / 2).ok_or_else(|| {
                Error::new(
                    ErrorCode::LimitExceeded,
                    format!(
                        "Model `{}` meshes to more triangles than the limit of {}",
                        model.name, limits.max_triangles
                    ),
                )
            })?;
        let mut x = MARGIN;
        let top = y + label_font::HEIGHT + LABEL_GAP;
        for frame in &frames {
            let mut view = render(frame, &opaque, &clear, palette, limits)?;
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

/// Darkens the pixels on voxel boundaries of a top view, each pixel once.
fn grid(view: &mut Image, model: &VoxelModel, scale: u32) {
    let [w, d, _] = model.size;
    let lines = |n: u32, size: u32| -> Vec<u32> {
        let mut lines: Vec<u32> = (0..=n).map(|i| (i * scale).min(size - 1)).collect();
        lines.dedup();
        lines
    };
    let columns = lines(w, view.width());
    let rows = lines(d, view.height());
    for y in 0..view.height() {
        for x in 0..view.width() {
            if columns.binary_search(&x).is_ok() || rows.binary_search(&y).is_ok() {
                if let Some(under) = view.pixel(x, y) {
                    view.set_pixel(x, y, blend(under, GRID));
                }
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
