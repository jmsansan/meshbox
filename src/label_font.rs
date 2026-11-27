//! A built-in 5×7 pixel font for the labels of preview sheets: uppercase letters,
//! digits, space and `_ - . : / ( ) x`. Lowercase letters are drawn as uppercase;
//! characters it lacks are drawn as `?`.

use crate::{Color, Image};

/// Width and height of a glyph, and the advance with the space after it.
pub(crate) const WIDTH: u32 = 5;
pub(crate) const HEIGHT: u32 = 7;
pub(crate) const ADVANCE: u32 = 6;

#[rustfmt::skip]
pub(crate) const GLYPHS: &[(char, [&str; 7])] = &[
    ('A', [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"]),
    ('B', ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."]),
    ('C', [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."]),
    ('D', ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."]),
    ('E', ["#####", "#....", "#....", "####.", "#....", "#....", "#####"]),
    ('F', ["#####", "#....", "#....", "####.", "#....", "#....", "#...."]),
    ('G', [".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".####"]),
    ('H', ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"]),
    ('I', [".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###."]),
    ('J', ["..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##.."]),
    ('K', ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"]),
    ('L', ["#....", "#....", "#....", "#....", "#....", "#....", "#####"]),
    ('M', ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#"]),
    ('N', ["#...#", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#"]),
    ('O', [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."]),
    ('P', ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."]),
    ('Q', [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#"]),
    ('R', ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"]),
    ('S', [".####", "#....", "#....", ".###.", "....#", "....#", "####."]),
    ('T', ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."]),
    ('U', ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."]),
    ('V', ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."]),
    ('W', ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "#.#.#", ".#.#."]),
    ('X', ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"]),
    ('Y', ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."]),
    ('Z', ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####"]),
    ('0', [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."]),
    ('1', ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."]),
    ('2', [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"]),
    ('3', ["#####", "...#.", "..#..", "...#.", "....#", "#...#", ".###."]),
    ('4', ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."]),
    ('5', ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."]),
    ('6', ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###."]),
    ('7', ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."]),
    ('8', [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."]),
    ('9', [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##.."]),
    (' ', [".....", ".....", ".....", ".....", ".....", ".....", "....."]),
    ('_', [".....", ".....", ".....", ".....", ".....", ".....", "#####"]),
    ('-', [".....", ".....", ".....", "#####", ".....", ".....", "....."]),
    ('.', [".....", ".....", ".....", ".....", ".....", ".##..", ".##.."]),
    (':', [".....", ".##..", ".##..", ".....", ".##..", ".##..", "....."]),
    ('/', [".....", "....#", "...#.", "..#..", ".#...", "#....", "....."]),
    ('(', ["...#.", "..#..", ".#...", ".#...", ".#...", "..#..", "...#."]),
    (')', [".#...", "..#..", "...#.", "...#.", "...#.", "..#..", ".#..."]),
    ('?', [".###.", "#...#", "....#", "...#.", "..#..", ".....", "..#.."]),
];

fn glyph(c: char) -> &'static [&'static str; 7] {
    let c = c.to_ascii_uppercase();
    GLYPHS
        .iter()
        .find(|(g, _)| *g == c)
        .or_else(|| GLYPHS.iter().find(|(g, _)| *g == '?'))
        .map(|(_, rows)| rows)
        .unwrap_or(&GLYPHS[0].1)
}

/// The width of `text` in pixels.
pub(crate) fn width(text: &str) -> u32 {
    let n = text.chars().count() as u32;
    if n == 0 {
        0
    } else {
        n * ADVANCE - (ADVANCE - WIDTH)
    }
}

/// Draws `text` with its top left at `(x, y)`.
pub(crate) fn draw(image: &mut Image, x: u32, y: u32, text: &str, color: Color) {
    for (i, c) in text.chars().enumerate() {
        let gx = x + i as u32 * ADVANCE;
        for (row, bits) in glyph(c).iter().enumerate() {
            for (col, bit) in bits.bytes().enumerate() {
                if bit == b'#' {
                    image.set_pixel(gx + col as u32, y + row as u32, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every glyph drawn in one row, as `#` and `.` text.
    fn drawn() -> String {
        let text: String = GLYPHS.iter().map(|(c, _)| *c).collect();
        let mut image = Image::new(width(&text), HEIGHT).unwrap();
        draw(&mut image, 0, 0, &text, Color::WHITE);
        let mut out = format!("{text}\n");
        for y in 0..HEIGHT {
            for x in 0..image.width() {
                let on = image.pixel(x, y).is_some_and(|c| c.a != 0);
                out.push(if on { '#' } else { '.' });
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn every_glyph_matches_the_fixture() {
        let fixture = include_str!("../tests/fixtures/label-font.txt");
        let drawn = drawn();
        if std::env::var_os("MESHBOX_BLESS").is_some() {
            std::fs::write("tests/fixtures/label-font.txt", &drawn).unwrap();
        }
        assert_eq!(drawn, fixture);
    }

    #[test]
    fn lowercase_is_drawn_as_uppercase_and_unknown_as_a_question_mark() {
        let mut a = Image::new(width("ab"), HEIGHT).unwrap();
        let mut b = Image::new(width("AB"), HEIGHT).unwrap();
        draw(&mut a, 0, 0, "ab", Color::WHITE);
        draw(&mut b, 0, 0, "AB", Color::WHITE);
        assert_eq!(a, b);
        let mut q = Image::new(WIDTH, HEIGHT).unwrap();
        let mut unknown = Image::new(WIDTH, HEIGHT).unwrap();
        draw(&mut q, 0, 0, "?", Color::WHITE);
        draw(&mut unknown, 0, 0, "é", Color::WHITE);
        assert_eq!(q, unknown);
    }
}
