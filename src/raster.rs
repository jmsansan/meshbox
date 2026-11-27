//! A CPU triangle rasterizer in fixed point, with a depth buffer: the same pixels on
//! every platform.

use crate::{Color, Image};

/// Subpixel precision: coordinates are in 1/256 of a pixel.
pub(crate) const SUBPIXEL: f32 = 256.0;

/// A screen vertex: `x` right and `y` down in 1/256 pixels, `z` toward the viewer
/// in the same units.
pub(crate) type Vertex = [i64; 3];

/// An image and its depth buffer.
pub(crate) struct Canvas {
    pub image: Image,
    depth: Vec<i64>,
}

/// How a triangle's color combines with what is drawn.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Mode {
    /// Replaces nearer pixels and writes depth.
    Opaque,
    /// Blends over nearer pixels by the color's alpha, without writing depth.
    Blend,
}

/// `(b - a) × (p - a)`: positive when `p` is on the inner side of edge `a → b` of a
/// triangle wound clockwise on screen (y down).
fn edge(a: Vertex, b: Vertex, px: i64, py: i64) -> i64 {
    (b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0])
}

/// Whether edge `a → b` of a clockwise triangle is a top or a left edge, which own
/// the pixel centers exactly on them, so that triangles sharing an edge cover each
/// pixel once.
fn top_left(a: Vertex, b: Vertex) -> bool {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    (dy == 0 && dx > 0) || dy < 0
}

impl Canvas {
    pub fn new(image: Image) -> Self {
        let n = image.width() as usize * image.height() as usize;
        Self {
            image,
            depth: vec![i64::MIN; n],
        }
    }

    /// Draws a triangle of one color.
    pub fn triangle(&mut self, v: [Vertex; 3], color: Color, mode: Mode) {
        let mut v = v;
        let area = edge(v[0], v[1], v[2][0], v[2][1]);
        if area == 0 {
            return;
        }
        if area < 0 {
            v.swap(1, 2);
        }
        let area = area.abs() as i128;
        let (w, h) = (self.image.width() as i64, self.image.height() as i64);
        let s = SUBPIXEL as i64;
        let min_x = v.iter().map(|p| p[0]).min().unwrap_or(0);
        let max_x = v.iter().map(|p| p[0]).max().unwrap_or(0);
        let min_y = v.iter().map(|p| p[1]).min().unwrap_or(0);
        let max_y = v.iter().map(|p| p[1]).max().unwrap_or(0);
        // Pixels whose centers, at (x + 1/2, y + 1/2), can be inside.
        let x0 = (min_x - s / 2 + s - 1).div_euclid(s).max(0);
        let x1 = (max_x - s / 2).div_euclid(s).min(w - 1);
        let y0 = (min_y - s / 2 + s - 1).div_euclid(s).max(0);
        let y1 = (max_y - s / 2).div_euclid(s).min(h - 1);
        let edges = [(1, 2), (2, 0), (0, 1)];
        let owns = edges.map(|(a, b)| top_left(v[a], v[b]));
        for py in y0..=y1 {
            let cy = py * s + s / 2;
            for px in x0..=x1 {
                let cx = px * s + s / 2;
                let mut weights = [0i64; 3];
                let mut inside = true;
                for (k, &(a, b)) in edges.iter().enumerate() {
                    let e = edge(v[a], v[b], cx, cy);
                    if e < 0 || (e == 0 && !owns[k]) {
                        inside = false;
                        break;
                    }
                    weights[k] = e;
                }
                if !inside {
                    continue;
                }
                let z = (weights[0] as i128 * v[0][2] as i128
                    + weights[1] as i128 * v[1][2] as i128
                    + weights[2] as i128 * v[2][2] as i128)
                    .div_euclid(area) as i64;
                let i = (py * w + px) as usize;
                if z <= self.depth[i] {
                    continue;
                }
                match mode {
                    Mode::Opaque => {
                        self.depth[i] = z;
                        self.image.set_pixel(px as u32, py as u32, color);
                    }
                    Mode::Blend => {
                        let under = self.image.pixel(px as u32, py as u32).unwrap_or_default();
                        self.image
                            .set_pixel(px as u32, py as u32, blend(under, color));
                    }
                }
            }
        }
    }
}

/// `over` drawn on `under` by `over`'s alpha, in integers.
pub(crate) fn blend(under: Color, over: Color) -> Color {
    let a = over.a as u32;
    let mix = |u: u8, o: u8| ((o as u32 * a + u as u32 * (255 - a) + 127) / 255) as u8;
    Color::rgba(
        mix(under.r, over.r),
        mix(under.g, over.g),
        mix(under.b, over.b),
        (a + under.a as u32 * (255 - a) / 255).min(255) as u8,
    )
}
