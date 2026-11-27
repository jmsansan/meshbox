//! The text format: definitions parsed from lines of words, then resolved.

pub(crate) mod lex;

use std::collections::HashMap;

use crate::document::Document;
use crate::palette::Palette;
use crate::voxel::VoxelModel;
use crate::{Color, Error, ErrorCode, Limits, Position};
use lex::{Node, Token};

type Parsed<T> = Result<T, Error>;

const DIRECTIVES: &str = "`palette` or `voxels`";

/// Directives of spritebox that meshbox does not read (#7).
const SPRITEBOX_DIRECTIVES: &[&str] = &[
    "sprite",
    "animation",
    "tileset",
    "tilemap",
    "font",
    "text",
    "states",
    "icons",
    "theme",
    "screen",
    "surface",
    "background",
    "texture",
    "lint",
    "group",
];

/// Characters that cannot be palette keys, besides spaces and control characters.
const RESERVED_KEYS: &[char] = &[';', '(', ')', ',', '='];

/// Parses `text` and returns the document, or every error found, ordered by
/// position.
pub(crate) fn parse(text: &str, limits: &Limits) -> Result<Document, Vec<Error>> {
    let mut errors = Vec::new();
    if text.len() > limits.max_text_bytes {
        return Err(vec![Error::at(
            Position::new(1, 1),
            ErrorCode::LimitExceeded,
            format!(
                "The text is {} bytes, more than the limit of {} bytes",
                text.len(),
                limits.max_text_bytes
            ),
        )]);
    }
    let nodes = lex::lex(text, 0, &mut errors);
    let mut parser = Parser {
        limits,
        errors,
        palettes: Vec::new(),
        models: Vec::new(),
        definitions: 0,
        voxels: 0,
    };
    for node in &nodes {
        parser.definitions += 1;
        if parser.definitions > limits.max_definitions {
            parser.errors.push(node.line.first().error(
                ErrorCode::LimitExceeded,
                format!(
                    "More definitions than the limit of {}",
                    limits.max_definitions
                ),
            ));
            break;
        }
        if let Err(e) = parser.definition(node) {
            parser.errors.push(e);
        }
    }
    let document = parser.resolve();
    let mut errors = parser.errors;
    if errors.is_empty() {
        Ok(document)
    } else {
        errors.sort_by_key(|e| e.position);
        Err(errors)
    }
}

/// A palette as written, before `from=` is resolved.
struct PaletteDef<'a> {
    name: Token<'a>,
    from: Option<Token<'a>>,
    colors: Vec<(char, Color)>,
}

/// A voxel model as written, before its keys are looked up in its palette.
struct ModelDef<'a> {
    name: Token<'a>,
    size: [u32; 3],
    palette: Token<'a>,
    origin: Option<[f32; 3]>,
    unit: f32,
    /// For each level that is written: its height and its rows.
    levels: Vec<(u32, Vec<Token<'a>>)>,
}

struct Parser<'a, 'l> {
    limits: &'l Limits,
    errors: Vec<Error>,
    palettes: Vec<PaletteDef<'a>>,
    models: Vec<ModelDef<'a>>,
    definitions: usize,
    /// Voxels of the models parsed so far, empty ones included.
    voxels: u64,
}

/// The name and the arguments of a definition's first line.
struct Header<'a> {
    name: Token<'a>,
    args: Vec<Token<'a>>,
    options: Vec<(Token<'a>, Token<'a>)>,
}

impl<'a> Header<'a> {
    fn new(node: &Node<'a>, what: &str) -> Parsed<Self> {
        let tokens = &node.line.tokens;
        let name = *tokens
            .get(1)
            .ok_or_else(|| tokens[0].end_error(format!("Expected a name for the {what}")))?;
        check_name(name)?;
        let mut args = Vec::new();
        let mut options = Vec::new();
        for &token in &tokens[2..] {
            match token.text.split_once('=') {
                Some((key, value)) if !key.is_empty() => {
                    let key_token = Token {
                        text: key,
                        pos: token.pos,
                    };
                    let value_token = Token {
                        text: value,
                        pos: token.at(key.chars().count() + 1),
                    };
                    if options.iter().any(|(k, _): &(Token, Token)| k.text == key) {
                        return Err(token
                            .error(ErrorCode::Syntax, format!("Option `{key}` is given twice")));
                    }
                    options.push((key_token, value_token));
                }
                _ => {
                    if !options.is_empty() {
                        return Err(token.error(
                            ErrorCode::Syntax,
                            format!("Unexpected `{}` after the options", token.text),
                        ));
                    }
                    args.push(token);
                }
            }
        }
        Ok(Self {
            name,
            args,
            options,
        })
    }

    /// Takes the value of option `key`; [`Header::finish`] rejects the ones not taken.
    fn option(&mut self, key: &str) -> Option<Token<'a>> {
        let index = self.options.iter().position(|(k, _)| k.text == key)?;
        Some(self.options.remove(index).1)
    }

    fn finish(self, what: &str, allowed: &str) -> Parsed<()> {
        if let Some((key, _)) = self.options.first() {
            return Err(key.error(
                ErrorCode::Syntax,
                format!(
                    "Unknown option `{}` for a {what}; expected {allowed}",
                    key.text
                ),
            ));
        }
        Ok(())
    }
}

impl Token<'_> {
    fn end_error(&self, message: String) -> Error {
        Error::at(self.end(), ErrorCode::Syntax, message)
    }
}

fn check_name(token: Token) -> Parsed<()> {
    let valid = token
        .text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid || token.text.is_empty() {
        return Err(token.error(
            ErrorCode::Syntax,
            format!(
                "Name `{}` may only use letters, digits, `_` and `-`",
                token.text
            ),
        ));
    }
    Ok(())
}

fn duplicate(name: Token, what: &str, first: Position) -> Error {
    name.error(
        ErrorCode::DuplicateName,
        format!(
            "{what} `{}` is already defined on line {}",
            name.text, first.line
        ),
    )
}

fn no_children(node: &Node, what: &str) -> Parsed<()> {
    match node.children.first() {
        Some(child) => Err(child.line.first().error(
            ErrorCode::Syntax,
            format!("Unexpected indented line under {what}"),
        )),
        None => Ok(()),
    }
}

/// Parses a size of `parts` numbers joined by `x`, such as `5x5x4`, each from 1 to
/// `max`.
fn size<const N: usize>(token: Token, max: u32, example: &str) -> Parsed<[u32; N]> {
    let invalid = || {
        token.error(
            ErrorCode::Syntax,
            format!("Expected a size such as `{example}`, not `{}`", token.text),
        )
    };
    let parts: Vec<&str> = token.text.split('x').collect();
    if parts.len() != N {
        return Err(invalid());
    }
    let mut out = [0u32; N];
    let mut offset = 0;
    for (slot, text) in out.iter_mut().zip(&parts) {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid());
        }
        let value: u64 = text.parse().unwrap_or(u64::MAX);
        if value == 0 {
            return Err(Error::at(
                token.at(offset),
                ErrorCode::InvalidArgument,
                "A size must be at least 1",
            ));
        }
        if value > max as u64 {
            return Err(Error::at(
                token.at(offset),
                ErrorCode::LimitExceeded,
                format!("Size {value} is more than the limit of {max}"),
            ));
        }
        *slot = value as u32;
        offset += text.len() + 1;
    }
    Ok(out)
}

/// A decimal number such as `2`, `-0.5` or `1.25`, as the nearest `f32`.
fn decimal(text: &str) -> Option<f32> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) = match digits.split_once('.') {
        Some((w, f)) => (w, Some(f)),
        None => (digits, None),
    };
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(whole) || fraction.is_some_and(|f| !all_digits(f)) || text.len() > 32 {
        return None;
    }
    text.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// `x,y,z` of decimals.
fn vector(token: Token) -> Parsed<[f32; 3]> {
    let parts: Vec<&str> = token.text.split(',').collect();
    let invalid = || {
        token.error(
            ErrorCode::Syntax,
            format!("Expected a vector such as `2,0,2`, not `{}`", token.text),
        )
    };
    if parts.len() != 3 {
        return Err(invalid());
    }
    let mut out = [0.0; 3];
    for (slot, part) in out.iter_mut().zip(parts) {
        *slot = decimal(part).ok_or_else(invalid)?;
    }
    Ok(out)
}

/// A positive length: a decimal such as `0.0625`, or a fraction such as `1/16`.
fn length(token: Token) -> Parsed<f32> {
    let value = match token.text.split_once('/') {
        Some((n, d)) => match (decimal(n), decimal(d)) {
            (Some(n), Some(d)) if d != 0.0 => Some(n / d),
            _ => None,
        },
        None => decimal(token.text),
    };
    match value {
        Some(v) if v > 0.0 && v.is_finite() => Ok(v),
        Some(_) => Err(token.error(
            ErrorCode::InvalidArgument,
            format!("A length must be more than 0, not `{}`", token.text),
        )),
        None => Err(token.error(
            ErrorCode::Syntax,
            format!(
                "Expected a length such as `0.0625` or `1/16`, not `{}`",
                token.text
            ),
        )),
    }
}

impl<'a> Parser<'a, '_> {
    fn definition(&mut self, node: &Node<'a>) -> Parsed<()> {
        match node.keyword() {
            "palette" => self.palette(node),
            "voxels" => self.voxels(node),
            other if SPRITEBOX_DIRECTIVES.contains(&other) => Err(node.line.first().error(
                ErrorCode::Syntax,
                format!(
                    "`{other}` is a spritebox directive; meshbox reads only the palettes of spritebox files"
                ),
            )),
            other => Err(node.line.first().error(
                ErrorCode::Syntax,
                format!("Unknown directive `{other}`; expected {DIRECTIVES}"),
            )),
        }
    }

    /// `palette NAME [from=OTHER] [lint=...]` with `KEY COLOR` and `ramp` lines, as
    /// spritebox writes them. `lint=` is spritebox's and has no effect here.
    fn palette(&mut self, node: &Node<'a>) -> Parsed<()> {
        let mut header = Header::new(node, "palette")?;
        if let Some(extra) = header.args.first() {
            return Err(extra.error(
                ErrorCode::Syntax,
                format!("Unexpected `{}` after the palette's name", extra.text),
            ));
        }
        let from = header.option("from");
        if let Some(from) = from {
            check_name(from)?;
        }
        let _lint = header.option("lint");
        let name = header.name;
        header.finish("palette", "`from=`")?;
        let mut colors: Vec<(char, Color)> = Vec::new();
        let mut ramps: Vec<(Token<'a>, Vec<char>)> = Vec::new();
        for child in &node.children {
            let result = if child.keyword() == "ramp" {
                self.ramp(child, name, &mut colors, &mut ramps)
            } else {
                self.palette_entry(child, name, &mut colors)
            };
            if let Err(e) = result {
                self.errors.push(e);
            }
        }
        self.palettes.push(PaletteDef { name, from, colors });
        Ok(())
    }

    fn palette_entry(
        &mut self,
        node: &Node<'a>,
        palette: Token<'a>,
        colors: &mut Vec<(char, Color)>,
    ) -> Parsed<()> {
        let tokens = &node.line.tokens;
        let key_token = tokens[0];
        no_children(node, "a palette color")?;
        let key = palette_key(key_token)?;
        let color_token = *tokens
            .get(1)
            .ok_or_else(|| key_token.end_error(format!("Expected a color after key `{key}`")))?;
        if let Some(extra) = tokens.get(2) {
            return Err(extra.error(
                ErrorCode::Syntax,
                format!("Unexpected `{}` after the color", extra.text),
            ));
        }
        // A key with a bad color stays defined, so that its uses are not reported as
        // unknown too.
        let color = match color_token.text.parse::<Color>() {
            Ok(c) => c,
            Err(e) => {
                self.errors.push(color_token.error(e.code, e.message));
                Color::TRANSPARENT
            }
        };
        if colors.iter().any(|&(k, _)| k == key) {
            return Err(key_token.error(
                ErrorCode::DuplicateName,
                format!("Key `{key}` is defined twice in palette `{}`", palette.text),
            ));
        }
        if colors.len() >= self.limits.max_palette_colors {
            return Err(key_token.error(
                ErrorCode::LimitExceeded,
                format!(
                    "Palette `{}` has more keys than the limit of {}",
                    palette.text, self.limits.max_palette_colors
                ),
            ));
        }
        colors.push((key, color));
        Ok(())
    }

    /// `ramp NAME KEYS [base=#rrggbb] [shift=DEGREES]`, spritebox's ramps: with a
    /// base, the ramp defines its keys with colors made around the base, exactly as
    /// spritebox makes them.
    fn ramp(
        &mut self,
        node: &Node<'a>,
        palette: Token<'a>,
        colors: &mut Vec<(char, Color)>,
        ramps: &mut Vec<(Token<'a>, Vec<char>)>,
    ) -> Parsed<()> {
        let tokens = &node.line.tokens;
        let usage = "write `ramp NAME KEYS`, such as `ramp skin abcd`, dark to light";
        let name = *tokens
            .get(1)
            .ok_or_else(|| tokens[0].end_error(format!("Expected a ramp's name; {usage}")))?;
        check_name(name)?;
        let keys = *tokens
            .get(2)
            .ok_or_else(|| name.end_error(format!("Expected the ramp's keys; {usage}")))?;
        if keys.text.contains('=') {
            return Err(keys.error(
                ErrorCode::Syntax,
                format!("Expected the ramp's keys before its options; {usage}"),
            ));
        }
        let (mut base, mut shift) = (None, None);
        for &token in &tokens[3..] {
            match token.text.split_once('=') {
                Some(("base", text)) if base.is_none() => {
                    base = Some(Token {
                        text,
                        pos: token.at("base=".len()),
                    });
                }
                Some(("shift", text)) if shift.is_none() => {
                    shift = Some(Token {
                        text,
                        pos: token.at("shift=".len()),
                    });
                }
                _ => {
                    return Err(token.error(
                        ErrorCode::Syntax,
                        format!(
                            "Unexpected `{}`; a ramp takes `base=#rrggbb` and `shift=DEGREES`",
                            token.text
                        ),
                    ));
                }
            }
        }
        no_children(node, "a ramp")?;
        if keys
            .text
            .chars()
            .nth(self.limits.max_palette_colors)
            .is_some()
        {
            return Err(keys.error(
                ErrorCode::LimitExceeded,
                format!(
                    "Ramp `{}` has more keys than the limit of {} a palette",
                    name.text, self.limits.max_palette_colors
                ),
            ));
        }
        let written: Vec<(char, Position)> = keys
            .text
            .chars()
            .enumerate()
            .map(|(i, c)| (c, keys.at(i)))
            .collect();
        if written.len() < 2 {
            return Err(keys.error(ErrorCode::InvalidArgument, "A ramp has at least two keys"));
        }
        for (i, &(key, pos)) in written.iter().enumerate() {
            if key.is_control() || RESERVED_KEYS.contains(&key) {
                return Err(Error::at(
                    pos,
                    ErrorCode::InvalidArgument,
                    format!("`{key}` cannot be a palette key"),
                ));
            }
            if written[..i].iter().any(|&(k, _)| k == key) {
                return Err(Error::at(
                    pos,
                    ErrorCode::DuplicateName,
                    format!("Key `{key}` is twice in ramp `{}`", name.text),
                ));
            }
            if let Some((other, _)) = ramps.iter().find(|(_, k)| k.contains(&key)) {
                return Err(Error::at(
                    pos,
                    ErrorCode::DuplicateName,
                    format!("Key `{key}` is in ramp `{}` already", other.text),
                ));
            }
        }
        if let Some((first, _)) = ramps.iter().find(|(r, _)| r.text == name.text) {
            return Err(duplicate(name, "Ramp", first.pos));
        }
        if let Some(shift) = shift {
            if base.is_none() {
                return Err(Error::at(
                    shift.pos,
                    ErrorCode::Syntax,
                    "`shift=` turns the hues of a ramp made from `base=`; give a base",
                ));
            }
        }
        if let Some(base) = base {
            let color = base
                .text
                .parse::<Color>()
                .ok()
                .filter(|c| c.is_opaque() && base.text.len() == 7)
                .ok_or_else(|| {
                    base.error(
                        ErrorCode::InvalidArgument,
                        format!("A ramp's base is an opaque `#rrggbb`, not `{}`", base.text),
                    )
                })?;
            let shift = match shift {
                Some(t) => t
                    .text
                    .parse::<u32>()
                    .ok()
                    .filter(|s| *s <= 60 && t.text.bytes().all(|b| b.is_ascii_digit()))
                    .ok_or_else(|| {
                        t.error(
                            ErrorCode::InvalidArgument,
                            format!("A shift is 0 to 60 degrees, not `{}`", t.text),
                        )
                    })?,
                None => 20,
            };
            if colors.len() + written.len() > self.limits.max_palette_colors {
                return Err(keys.error(
                    ErrorCode::LimitExceeded,
                    format!(
                        "Palette `{}` has more keys than the limit of {}",
                        palette.text, self.limits.max_palette_colors
                    ),
                ));
            }
            let made = crate::oklab::ramp(color, written.len(), shift);
            for (&(key, pos), color) in written.iter().zip(made) {
                if colors.iter().any(|&(k, _)| k == key) {
                    return Err(Error::at(
                        pos,
                        ErrorCode::DuplicateName,
                        format!("Key `{key}` is defined twice in palette `{}`", palette.text),
                    ));
                }
                colors.push((key, color));
            }
        }
        ramps.push((name, written.iter().map(|&(k, _)| k).collect()));
        Ok(())
    }

    /// `voxels NAME WxDxH palette=P [origin=x,y,z] [unit=U]` with `level N` blocks.
    fn voxels(&mut self, node: &Node<'a>) -> Parsed<()> {
        let mut header = Header::new(node, "voxel model")?;
        let name = header.name;
        let size_token = *header.args.first().ok_or_else(|| {
            name.end_error("Expected the model's size after its name, such as `5x5x4`".to_owned())
        })?;
        if let Some(extra) = header.args.get(1) {
            return Err(extra.error(
                ErrorCode::Syntax,
                format!("Unexpected `{}` after the model's size", extra.text),
            ));
        }
        let size: [u32; 3] = size(size_token, self.limits.max_voxel_side, "5x5x4")?;
        let palette = header.option("palette").ok_or_else(|| {
            node.line.tokens[node.line.tokens.len() - 1]
                .end_error("Expected `palette=NAME`: the palette of the model's keys".to_owned())
        })?;
        check_name(palette)?;
        let origin = header.option("origin").map(vector).transpose()?;
        let unit = match header.option("unit") {
            Some(t) => length(t)?,
            None => 1.0 / 16.0,
        };
        header.finish("voxel model", "`palette=`, `origin=` or `unit=`")?;
        // The voxels are counted, and refused, before memory for them is allocated.
        let cells = size.iter().map(|&s| s as u64).product::<u64>();
        self.voxels += cells;
        if self.voxels > self.limits.max_voxels {
            return Err(size_token.error(
                ErrorCode::LimitExceeded,
                format!(
                    "With model `{}`, the document has more voxels than the limit of {}",
                    name.text, self.limits.max_voxels
                ),
            ));
        }
        let [width, depth, height] = size;
        let mut levels: Vec<(u32, Vec<Token<'a>>)> = Vec::new();
        let mut level_lines: Vec<(u32, Position)> = Vec::new();
        for child in &node.children {
            let tokens = &child.line.tokens;
            if child.keyword() != "level" {
                self.errors.push(tokens[0].error(
                    ErrorCode::Syntax,
                    format!(
                        "Unexpected `{}` in a voxel model; expected `level N`",
                        tokens[0].text
                    ),
                ));
                continue;
            }
            let Some(&number) = tokens.get(1) else {
                self.errors.push(
                    tokens[0].end_error("Expected the level's height, such as `level 0`".into()),
                );
                continue;
            };
            if let Some(extra) = tokens.get(2) {
                self.errors.push(extra.error(
                    ErrorCode::Syntax,
                    format!("Unexpected `{}` after the level's height", extra.text),
                ));
                continue;
            }
            let Some(y) = number
                .text
                .parse::<u32>()
                .ok()
                .filter(|_| number.text.bytes().all(|b| b.is_ascii_digit()))
            else {
                self.errors.push(number.error(
                    ErrorCode::Syntax,
                    format!(
                        "Expected a level's height, such as `0`, not `{}`",
                        number.text
                    ),
                ));
                continue;
            };
            if y >= height {
                self.errors.push(number.error(
                    ErrorCode::InvalidArgument,
                    format!(
                        "Level {y} is outside model `{}`, whose levels are 0 to {}",
                        name.text,
                        height - 1
                    ),
                ));
                continue;
            }
            if let Some(&(_, first)) = level_lines.iter().find(|(l, _)| *l == y) {
                self.errors.push(number.error(
                    ErrorCode::DuplicateName,
                    format!("Level {y} is already written on line {}", first.line),
                ));
                continue;
            }
            level_lines.push((y, number.pos));
            match self.level(child, width, depth) {
                Ok(rows) => levels.push((y, rows)),
                Err(e) => self.errors.push(e),
            }
        }
        self.models.push(ModelDef {
            name,
            size,
            palette,
            origin,
            unit,
            levels,
        });
        Ok(())
    }

    /// The rows of a level: `depth` rows of `width` keys.
    fn level(&mut self, node: &Node<'a>, width: u32, depth: u32) -> Parsed<Vec<Token<'a>>> {
        let mut rows = Vec::new();
        for child in &node.children {
            let tokens = &child.line.tokens;
            if let Some(extra) = tokens.get(1) {
                return Err(extra.error(
                    ErrorCode::Syntax,
                    "A row of voxels is one word of keys, without spaces",
                ));
            }
            no_children(child, "a row of voxels")?;
            let row = tokens[0];
            let count = row.text.chars().count();
            if count != width as usize {
                let at = if count > width as usize {
                    row.at(width as usize)
                } else {
                    row.end()
                };
                return Err(Error::at(
                    at,
                    ErrorCode::SizeMismatch,
                    format!("A row has {count} voxels; the model is {width} wide"),
                ));
            }
            if rows.len() == depth as usize {
                return Err(row.error(
                    ErrorCode::SizeMismatch,
                    format!("A level has more than {depth} rows; the model is {depth} deep"),
                ));
            }
            rows.push(row);
        }
        if rows.len() != depth as usize {
            let level = node.line.first();
            return Err(level.error(
                ErrorCode::SizeMismatch,
                format!(
                    "The level has {} rows; the model is {depth} deep",
                    rows.len()
                ),
            ));
        }
        Ok(rows)
    }

    /// Resolves palettes, then models; errors go to `self.errors`.
    fn resolve(&mut self) -> Document {
        let palettes = self.resolve_palettes();
        // Only the palettes that resolved stay; models refer to them by their index
        // among those, found by name.
        let mut index: HashMap<&str, usize> = HashMap::new();
        let mut kept = Vec::new();
        for (i, p) in palettes.into_iter().enumerate() {
            if let Some(p) = p {
                index.insert(self.palettes[i].name.text, kept.len());
                kept.push(p);
            }
        }
        let mut models: Vec<VoxelModel> = Vec::new();
        let mut names: HashMap<&str, Position> = HashMap::new();
        for def in &self.models {
            if let Some(&first) = names.get(def.name.text) {
                self.errors.push(duplicate(def.name, "Model", first));
                continue;
            }
            names.insert(def.name.text, def.name.pos);
            let Some(&p) = index.get(def.palette.text) else {
                // A palette with errors of its own has them reported already.
                if !self
                    .palettes
                    .iter()
                    .any(|d| d.name.text == def.palette.text)
                {
                    self.errors.push(def.palette.error(
                        ErrorCode::UnknownName,
                        format!("Unknown palette `{}`", def.palette.text),
                    ));
                }
                continue;
            };
            let palette = &kept[p];
            let keys: Vec<char> = palette.keys.iter().map(|&(k, _)| k).collect();
            let [w, d, _] = def.size;
            let mut cells = vec![0u16; def.size.iter().map(|&s| s as usize).product()];
            let mut ok = true;
            for (y, rows) in &def.levels {
                for (z, row) in rows.iter().enumerate() {
                    for (x, key) in row.text.chars().enumerate() {
                        if key == '.' {
                            continue;
                        }
                        let Some(k) = keys.iter().position(|&c| c == key) else {
                            self.errors.push(Error::at(
                                row.at(x),
                                ErrorCode::UnknownName,
                                format!("Key `{key}` is not in palette `{}`", palette.name),
                            ));
                            ok = false;
                            continue;
                        };
                        // A fully transparent key is empty space, as `.` is.
                        if palette.keys[k].1.a == 0 {
                            continue;
                        }
                        let i = ((*y as usize * d as usize) + z) * w as usize + x;
                        cells[i] = k as u16 + 1;
                    }
                }
            }
            if !ok {
                continue;
            }
            let origin =
                def.origin
                    .unwrap_or([def.size[0] as f32 / 2.0, 0.0, def.size[1] as f32 / 2.0]);
            models.push(VoxelModel {
                name: def.name.text.to_owned(),
                size: def.size,
                palette: p,
                origin,
                unit: def.unit,
                cells,
                keys,
            });
        }
        Document {
            palettes: kept,
            models,
        }
    }

    /// Resolves `from=`: a palette starts with the colors of the one it names, then
    /// replaces or adds its own. Returns `None` for palettes with errors.
    fn resolve_palettes(&mut self) -> Vec<Option<Palette>> {
        let mut index: HashMap<&str, usize> = HashMap::new();
        let mut usable = vec![true; self.palettes.len()];
        for (i, def) in self.palettes.iter().enumerate() {
            if let Some(&first) = index.get(def.name.text) {
                self.errors.push(duplicate(
                    def.name,
                    "Palette",
                    self.palettes[first].name.pos,
                ));
                usable[i] = false;
            } else {
                index.insert(def.name.text, i);
            }
        }
        let mut resolved: Vec<Option<Palette>> = vec![None; self.palettes.len()];
        let mut failed = vec![false; self.palettes.len()];
        for start in 0..self.palettes.len() {
            if !usable[start] {
                continue;
            }
            // The chain of `from=` from `start` to a palette without one, or to one
            // already resolved or failed.
            let mut chain = vec![start];
            let mut error = None;
            loop {
                let current = *chain.last().unwrap_or(&start);
                if resolved[current].is_some() || failed[current] {
                    break;
                }
                let def = &self.palettes[current];
                let Some(from) = def.from else { break };
                let Some(&next) = index.get(from.text) else {
                    error = Some(from.error(
                        ErrorCode::UnknownName,
                        format!("Unknown palette `{}`", from.text),
                    ));
                    break;
                };
                if chain.contains(&next) {
                    error = Some(from.error(
                        ErrorCode::Cycle,
                        format!(
                            "Palette `{}` is copied from itself through `from=`",
                            def.name.text
                        ),
                    ));
                    break;
                }
                if chain.len() > self.limits.max_nesting {
                    error = Some(from.error(
                        ErrorCode::LimitExceeded,
                        format!(
                            "Palettes copied `from=` more than the limit of {} deep",
                            self.limits.max_nesting
                        ),
                    ));
                    break;
                }
                chain.push(next);
            }
            if let Some(e) = error {
                if !failed[*chain.last().unwrap_or(&start)] {
                    self.errors.push(e);
                }
                for &i in &chain {
                    if resolved[i].is_none() {
                        failed[i] = true;
                    }
                }
                continue;
            }
            // Resolve from the end of the chain back to `start`.
            for &i in chain.iter().rev() {
                if resolved[i].is_some() || failed[i] {
                    continue;
                }
                let def = &self.palettes[i];
                let mut keys: Vec<(char, Color)> = match def.from {
                    Some(from) => {
                        let parent = index[from.text];
                        match &resolved[parent] {
                            Some(p) => p.keys.clone(),
                            None => {
                                failed[i] = true;
                                continue;
                            }
                        }
                    }
                    None => Vec::new(),
                };
                for &(key, color) in &def.colors {
                    match keys.iter_mut().find(|(k, _)| *k == key) {
                        Some(slot) => slot.1 = color,
                        None => keys.push((key, color)),
                    }
                }
                if keys.len() > self.limits.max_palette_colors {
                    self.errors.push(def.name.error(
                        ErrorCode::LimitExceeded,
                        format!(
                            "Palette `{}` has {} keys with those it copies, more than the limit of {}",
                            def.name.text,
                            keys.len(),
                            self.limits.max_palette_colors
                        ),
                    ));
                    failed[i] = true;
                    continue;
                }
                resolved[i] = Some(Palette {
                    name: def.name.text.to_owned(),
                    keys,
                });
            }
        }
        resolved
    }
}

/// A palette line's key: one character that is not reserved.
fn palette_key(token: Token) -> Parsed<char> {
    let mut chars = token.text.chars();
    let key = chars.next().unwrap_or(' ');
    if chars.next().is_some() {
        return Err(token.error(
            ErrorCode::Syntax,
            format!(
                "A palette key is one character, not `{}`; write `KEY COLOR`",
                token.text
            ),
        ));
    }
    if key.is_control() || RESERVED_KEYS.contains(&key) {
        return Err(token.error(
            ErrorCode::InvalidArgument,
            format!("`{key}` cannot be a palette key"),
        ));
    }
    Ok(key)
}
