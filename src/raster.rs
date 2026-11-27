//! A CPU triangle rasterizer in fixed point, with a depth buffer: the same pixels on
//! every platform.
//!
//! Vertices are integers in 1/256 of a pixel, and every test is exact integer
//! arithmetic, so triangles that share an edge, or a point on an edge, cover each
//! pixel once. Each row is scanned only across the span the triangle covers, so the
//! work follows the pixels drawn, not the triangle's bounding box.

use crate::{Color, Image};

/// Subpixel precision: coordinates are in 1/256 of a pixel.
pub(crate) const SUBPIXEL: i64 = 256;

/// A screen vertex: `x` right and `y` down in 1/256 pixels, `z` toward the viewer.
pub(crate) type Vertex = [i64; 3];

/// A translucent fragment: the pixel's index, its depth, and its color.
pub(crate) type Fragment = (usize, i64, Color);

/// An image and its depth buffer.
pub(crate) struct Canvas {
    pub image: Image,
    depth: Vec<i64>,
}

/// `(b - a) × (p - a)`: positive when `p` is on the inner side of edge `a → b` of a
/// triangle wound clockwise on screen (y down).
fn edge(a: Vertex, b: Vertex, px: i64, py: i64) -> i128 {
    (b[0] - a[0]) as i128 * (py - a[1]) as i128 - (b[1] - a[1]) as i128 * (px - a[0]) as i128
}

/// Whether edge `a → b` of a clockwise triangle is a top or a left edge, which own
/// the pixel centers exactly on them.
fn top_left(a: Vertex, b: Vertex) -> bool {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    (dy == 0 && dx > 0) || dy < 0
}

/// The pixel centers `(px, py, depth)` a triangle covers, in row order.
fn cover(v: [Vertex; 3], width: i64, height: i64, mut visit: impl FnMut(i64, i64, i64)) {
    let mut v = v;
    let area = edge(v[0], v[1], v[2][0], v[2][1]);
    if area == 0 {
        return;
    }
    if area < 0 {
        v.swap(1, 2);
    }
    let area = area.abs();
    let s = SUBPIXEL;
    let min_y = v.iter().map(|p| p[1]).min().unwrap_or(0);
    let max_y = v.iter().map(|p| p[1]).max().unwrap_or(0);
    let y0 = (min_y - s / 2 + s - 1).div_euclid(s).max(0);
    let y1 = (max_y - s / 2).div_euclid(s).min(height - 1);
    let edges = [(1, 2), (2, 0), (0, 1)];
    let owns = edges.map(|(a, b)| top_left(v[a], v[b]));
    for py in y0..=y1 {
        let cy = py * s + s / 2;
        // Each edge is linear in x along the row: e(cx) = c - d·cx. The inside of all
        // three is one span of x, found exactly and widened by a pixel for safety;
        // the exact tests below decide each pixel.
        let (mut lo, mut hi) = (0i64, width - 1);
        for &(a, b) in &edges {
            let (pa, pb) = (v[a], v[b]);
            let d = (pb[1] - pa[1]) as i128;
            let c = (pb[0] - pa[0]) as i128 * (cy - pa[1]) as i128 + d * pa[0] as i128;
            if d > 0 {
                // cx ≤ c / d
                let bound = c.div_euclid(d);
                hi = hi.min(pixel_at_or_below(bound) + 1);
            } else if d < 0 {
                // cx ≥ c / d, rounded up: -floor(c / -d).
                let bound = -(c.div_euclid(-d));
                lo = lo.max(pixel_at_or_above(bound) - 1);
            } else if c < 0 {
                lo = 1;
                hi = 0;
            }
        }
        for px in lo.max(0)..=hi.min(width - 1) {
            let cx = px * s + s / 2;
            let mut weights = [0i128; 3];
            let mut inside = true;
            for (k, &(a, b)) in edges.iter().enumerate() {
                let e = edge(v[a], v[b], cx, cy);
                if e < 0 || (e == 0 && !owns[k]) {
                    inside = false;
                    break;
                }
                weights[k] = e;
            }
            if inside {
                let z = (weights[0] * v[0][2] as i128
                    + weights[1] * v[1][2] as i128
                    + weights[2] * v[2][2] as i128)
                    .div_euclid(area) as i64;
                visit(px, py, z);
            }
        }
    }
}

/// The last pixel whose center is at or left of `x` (in subpixels).
fn pixel_at_or_below(x: i128) -> i64 {
    let s = SUBPIXEL as i128;
    (x - s / 2)
        .div_euclid(s)
        .clamp(i64::MIN as i128 / 2, i64::MAX as i128 / 2) as i64
}

/// The first pixel whose center is at or right of `x` (in subpixels).
fn pixel_at_or_above(x: i128) -> i64 {
    let s = SUBPIXEL as i128;
    (x - s / 2 + s - 1)
        .div_euclid(s)
        .clamp(i64::MIN as i128 / 2, i64::MAX as i128 / 2) as i64
}

impl Canvas {
    pub fn new(image: Image) -> Self {
        let n = image.width() as usize * image.height() as usize;
        Self {
            image,
            depth: vec![i64::MIN; n],
        }
    }

    /// Draws an opaque triangle where it is nearer than what is drawn.
    pub fn opaque(&mut self, v: [Vertex; 3], color: Color) {
        let (w, h) = (self.image.width() as i64, self.image.height() as i64);
        let (depth, image) = (&mut self.depth, &mut self.image);
        cover(v, w, h, |px, py, z| {
            let i = (py * w + px) as usize;
            if z > depth[i] {
                depth[i] = z;
                image.set_pixel(px as u32, py as u32, color);
            }
        });
    }

    /// Collects the fragments of a translucent triangle that are not behind the
    /// opaque surface; at equal depth the glass is in front, as it is where a
    /// translucent face meets an opaque one. Returns false, having stopped, once
    /// `out` would hold more than `max` fragments.
    pub fn fragments(
        &self,
        v: [Vertex; 3],
        color: Color,
        out: &mut Vec<Fragment>,
        max: usize,
    ) -> bool {
        let (w, h) = (self.image.width() as i64, self.image.height() as i64);
        let mut room = true;
        cover(v, w, h, |px, py, z| {
            let i = (py * w + px) as usize;
            if room && z >= self.depth[i] {
                if out.len() >= max {
                    room = false;
                } else {
                    out.push((i, z, color));
                }
            }
        });
        room
    }

    /// Blends translucent fragments, at each pixel from the farthest to the nearest.
    pub fn blend_fragments(&mut self, mut fragments: Vec<Fragment>) {
        fragments.sort_by_key(|&(i, z, _)| (i, z));
        let w = self.image.width() as usize;
        for (i, _, color) in fragments {
            let (x, y) = ((i % w) as u32, (i / w) as u32);
            let under = self.image.pixel(x, y).unwrap_or_default();
            self.image.set_pixel(x, y, blend(under, color));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragments_blend_far_to_near_whatever_the_order_found() {
        let red = Color::rgba(255, 0, 0, 128);
        let green = Color::rgba(0, 255, 0, 128);
        let mut a = Canvas::new(Image::new(1, 1).unwrap());
        let mut b = Canvas::new(Image::new(1, 1).unwrap());
        // Red is nearer (depth 5) than green (depth 2).
        a.blend_fragments(vec![(0, 5, red), (0, 2, green)]);
        b.blend_fragments(vec![(0, 2, green), (0, 5, red)]);
        assert_eq!(a.image, b.image);
        let c = a.image.pixel(0, 0).unwrap();
        assert!(c.r > c.g, "the nearer red is on top: {c:?}");
    }

    #[test]
    fn triangles_sharing_an_edge_cover_each_pixel_once() {
        let mut canvas = Canvas::new(Image::new(16, 16).unwrap());
        let s = SUBPIXEL;
        let (a, b, c, d) = (
            [0, 0, 0],
            [16 * s, 0, 0],
            [16 * s, 16 * s, 0],
            [0, 16 * s, 0],
        );
        let mut count = vec![0u8; 256];
        for t in [[a, b, c], [a, c, d]] {
            cover(t, 16, 16, |x, y, _| count[(y * 16 + x) as usize] += 1);
        }
        assert!(count.iter().all(|&n| n == 1), "{count:?}");
        canvas.opaque([a, b, c], Color::WHITE);
    }
}
