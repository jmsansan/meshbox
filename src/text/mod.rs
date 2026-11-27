//! The text format: definitions parsed from lines of words, then resolved.

pub(crate) mod lex;

use std::collections::HashMap;

use crate::document::Document;
use crate::error::Errors;
use crate::palette::Palette;
use crate::voxel::VoxelModel;
use crate::{Color, Error, ErrorCode, Limits, Position};
use lex::{Node, Token};

type Parsed<T> = Result<T, Error>;

const DIRECTIVES: &str = "`palette` or `voxels`";

/// More options than any directive takes, so that checking for duplicates stays cheap.
const MAX_OPTIONS: usize = 16;

/// The longest name, so that messages and lookups that copy names stay small. The one
/// rule of palettes that spritebox does not share: it takes longer names.
const MAX_NAME: usize = 128;

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
    let mut errors = Errors::default();
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
    let nodes = lex::lex(text, 0, limits.max_definitions, &mut errors);
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
    let mut errors = parser.errors.into_vec();
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
    ramps: Vec<RampDef<'a>>,
}

/// A `ramp` line: its name, and its keys with their positions.
type RampDef<'a> = (Token<'a>, Vec<(char, Position)>);

/// A voxel model as written, before its keys are looked up in its palette.
struct ModelDef<'a> {
    name: Token<'a>,
    size: [u32; 3],
    palette: Token<'a>,
    origin: Option<[f32; 3]>,
    unit: f32,
    /// For each level that is written: its height and its rows.
    levels: Vec<(u32, Vec<(usize, Token<'a>)>)>,
}

struct Parser<'a, 'l> {
    limits: &'l Limits,
    errors: Errors,
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
                    if options.len() == MAX_OPTIONS {
                        return Err(token.error(
                            ErrorCode::Syntax,
                            format!("More than {MAX_OPTIONS} options"),
                        ));
                    }
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
    if token.text.len() > MAX_NAME {
        return Err(token.error(
            ErrorCode::InvalidArgument,
            format!("A name is at most {MAX_NAME} characters"),
        ));
    }
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
        if let Some(lint) = header.option("lint") {
            lint_option(lint)?;
        }
        let name = header.name;
        header.finish("palette", "`from=`")?;
        let mut colors: Vec<(char, Color)> = Vec::new();
        let mut ramps: Vec<RampDef<'a>> = Vec::new();
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
        self.palettes.push(PaletteDef {
            name,
            from,
            colors,
            ramps,
        });
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
        ramps: &mut Vec<RampDef<'a>>,
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
        // Ramps share no keys and name only palette keys, so all of a palette's ramps
        // have at most as many keys as a palette: past that, the ramp is refused before
        // any work on its keys, which keeps checking them against other ramps cheap.
        let used: usize = ramps.iter().map(|(_, k)| k.len()).sum();
        let room = self.limits.max_palette_colors.saturating_sub(used);
        if keys.text.chars().nth(room).is_some() {
            return Err(keys.error(
                ErrorCode::LimitExceeded,
                format!(
                    "With ramp `{}`, the palette's ramps have more keys than the limit of {}",
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
            if let Some((other, _)) = ramps.iter().find(|(_, k)| k.iter().any(|&(c, _)| c == key)) {
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
        ramps.push((name, written));
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
        let cells = size.iter().fold(0u64, |acc, &s| {
            if acc == 0 {
                s as u64
            } else {
                acc.saturating_mul(s as u64)
            }
        });
        self.voxels = self.voxels.saturating_add(cells);
        // Two bytes a cell must fit in memory too, whatever the limits say.
        let fits = usize::try_from(cells)
            .ok()
            .and_then(|c| c.checked_mul(2))
            .is_some_and(|bytes| bytes <= isize::MAX as usize);
        if !fits {
            return Err(size_token.error(
                ErrorCode::LimitExceeded,
                format!(
                    "Model `{}` has more voxels than this platform can hold",
                    name.text
                ),
            ));
        }
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
        let mut levels: Vec<(u32, Vec<(usize, Token<'a>)>)> = Vec::new();
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
            let rows = self.level(child, width, depth);
            levels.push((y, rows));
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

    /// The rows of a level, `depth` rows of `width` keys, each with its `z`. Errors go
    /// to `self.errors`, and every row is returned, of the right width or not, so that
    /// their keys are still checked.
    fn level(&mut self, node: &Node<'a>, width: u32, depth: u32) -> Vec<(usize, Token<'a>)> {
        let mut rows = Vec::new();
        let mut count = 0;
        for child in &node.children {
            let tokens = &child.line.tokens;
            count += 1;
            if count == depth as usize + 1 {
                self.errors.push(tokens[0].error(
                    ErrorCode::SizeMismatch,
                    format!("A level has more than {depth} rows; the model is {depth} deep"),
                ));
            }
            if let Some(extra) = tokens.get(1) {
                self.errors.push(extra.error(
                    ErrorCode::Syntax,
                    "A row of voxels is one word of keys, without spaces",
                ));
                continue;
            }
            if let Err(e) = no_children(child, "a row of voxels") {
                self.errors.push(e);
            }
            let row = tokens[0];
            let length = row.text.chars().count();
            if length != width as usize {
                let at = if length > width as usize {
                    row.at(width as usize)
                } else {
                    row.end()
                };
                self.errors.push(Error::at(
                    at,
                    ErrorCode::SizeMismatch,
                    format!("A row has {length} voxels; the model is {width} wide"),
                ));
            }
            // Kept whatever its width, so that its keys are checked too.
            rows.push((count - 1, row));
        }
        if count < depth as usize {
            self.errors.push(node.line.first().error(
                ErrorCode::SizeMismatch,
                format!("The level has {count} rows; the model is {depth} deep"),
            ));
        }
        rows
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
            // Cells hold a key's index plus one in 16 bits.
            if palette.keys.len() >= u16::MAX as usize {
                self.errors.push(def.palette.error(
                    ErrorCode::LimitExceeded,
                    format!(
                        "Palette `{}` has more than {} keys, the most a model can use",
                        palette.name,
                        u16::MAX - 1
                    ),
                ));
                continue;
            }
            let keys: Vec<char> = palette.keys.iter().map(|&(k, _)| k).collect();
            let [w, d, _] = def.size;
            let mut cells = vec![0u16; def.size.iter().map(|&s| s as usize).product()];
            let mut ok = true;
            for (y, rows) in &def.levels {
                for &(z, row) in rows {
                    for (x, key) in row.text.chars().enumerate() {
                        if key == '.' {
                            continue;
                        }
                        let Some(k) = keys.iter().position(|&c| c == key) else {
                            // Past the cap, the message is not even formatted.
                            if self.errors.keeps(Some(row.at(x))) {
                                self.errors.push(Error::at(
                                    row.at(x),
                                    ErrorCode::UnknownName,
                                    format!("Key `{key}` is not in palette `{}`", palette.name),
                                ));
                            }
                            ok = false;
                            continue;
                        };
                        // A fully transparent key is empty space, as `.` is.
                        if palette.keys[k].1.a == 0 {
                            continue;
                        }
                        // Rows past the depth and keys past the width are errors already;
                        // their keys are still checked.
                        if z >= d as usize || x >= w as usize {
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

    /// Resolves `from=`: a palette starts with the colors and ramps of the one it
    /// names, then replaces or adds its own. Returns `None` for palettes with errors.
    fn resolve_palettes(&mut self) -> Vec<Option<Palette>> {
        let count = self.palettes.len();
        let mut index: HashMap<&str, usize> = HashMap::new();
        let mut state = vec![State::Pending; count];
        for (i, def) in self.palettes.iter().enumerate() {
            if let Some(&first) = index.get(def.name.text) {
                self.errors.push(duplicate(
                    def.name,
                    "Palette",
                    self.palettes[first].name.pos,
                ));
                state[i] = State::Failed;
            } else {
                index.insert(def.name.text, i);
            }
        }
        let mut resolved: Vec<Option<Palette>> = vec![None; count];
        // How many `from=` steps lead from each resolved palette to one without.
        let mut depth = vec![0usize; count];
        for start in 0..count {
            // The chain of `from=` from `start` to a palette that is resolved, failed,
            // or copies none; palettes are then resolved from its end back.
            let mut chain = vec![start];
            let mut error = None;
            loop {
                let current = chain[chain.len() - 1];
                if state[current] != State::Pending {
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
                chain.push(next);
            }
            if let Some(e) = error {
                self.errors.push(e);
                for &i in &chain {
                    if state[i] == State::Pending {
                        state[i] = State::Failed;
                    }
                }
                continue;
            }
            for n in (0..chain.len()).rev() {
                let i = chain[n];
                if state[i] != State::Pending {
                    continue;
                }
                let def = &self.palettes[i];
                let parent = def.from.map(|from| index[from.text]);
                if let Some(p) = parent {
                    if state[p] == State::Failed {
                        // The palette copied has errors of its own, reported already.
                        for &j in &chain[..=n] {
                            state[j] = State::Failed;
                        }
                        break;
                    }
                    depth[i] = depth[p] + 1;
                    if depth[i] > self.limits.max_nesting {
                        self.errors.push(def.from.unwrap_or(def.name).error(
                            ErrorCode::LimitExceeded,
                            format!(
                                "Palettes copied `from=` more than the limit of {} deep",
                                self.limits.max_nesting
                            ),
                        ));
                        for &j in &chain[..=n] {
                            state[j] = State::Failed;
                        }
                        break;
                    }
                }
                let (mut keys, mut ramps) = match parent.and_then(|p| resolved[p].as_ref()) {
                    Some(p) => (p.keys.clone(), p.ramps.clone()),
                    None => (Vec::new(), Vec::new()),
                };
                for &(key, color) in &def.colors {
                    match keys.iter_mut().find(|(k, _)| *k == key) {
                        Some(slot) => slot.1 = color,
                        None => keys.push((key, color)),
                    }
                }
                // A ramp of the palette copied is replaced by one of the same name; the
                // others keep their keys, which no ramp written here may share.
                let replaced: Vec<&str> = def.ramps.iter().map(|(r, _)| r.text).collect();
                for (name, ramp_keys) in &def.ramps {
                    let shared = ramp_keys.iter().find_map(|&(key, pos)| {
                        ramps
                            .iter()
                            .filter(|(r, _)| !replaced.contains(&r.as_str()))
                            .find(|(_, k)| k.contains(&key))
                            .map(|(r, _)| (key, pos, r.clone()))
                    });
                    if let Some((key, pos, other)) = shared {
                        self.errors.push(Error::at(
                            pos,
                            ErrorCode::DuplicateName,
                            format!(
                                "Key `{key}` is in ramp `{other}` already, which palette `{}` copies",
                                def.name.text
                            ),
                        ));
                        continue;
                    }
                    let missing = ramp_keys
                        .iter()
                        .find(|(k, _)| !keys.iter().any(|(c, _)| c == k));
                    if let Some(&(key, pos)) = missing {
                        self.errors.push(Error::at(
                            pos,
                            ErrorCode::UnknownName,
                            format!(
                                "Key `{key}` of ramp `{}` is not in palette `{}`",
                                name.text, def.name.text
                            ),
                        ));
                        continue;
                    }
                    let made = (
                        name.text.to_owned(),
                        ramp_keys.iter().map(|&(k, _)| k).collect(),
                    );
                    match ramps.iter_mut().find(|(r, _)| *r == made.0) {
                        Some(old) => *old = made,
                        None => ramps.push(made),
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
                    for &j in &chain[..=n] {
                        state[j] = State::Failed;
                    }
                    break;
                }
                state[i] = State::Resolved;
                resolved[i] = Some(Palette {
                    name: def.name.text.to_owned(),
                    keys,
                    ramps,
                });
            }
        }
        resolved
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Pending,
    Resolved,
    Failed,
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

/// spritebox's lint codes, which `lint=allow(CODE,...)` on a palette may name.
const LINT_CODES: &[&str] = &[
    "orphan_pixel",
    "jaggy",
    "double_outline",
    "stray_alpha",
    "pillow_shading",
    "banding",
    "mixel",
    "too_many_colors",
    "inconsistent_size",
    "inconsistent_outline",
    "inconsistent_palette",
    "inconsistent_scale",
    "near_duplicate_color",
    "flat_ramp",
    "low_contrast",
    "unused_key",
    "screen_bounds",
    "screen_overlap",
    "off_grid",
    "background_clipped",
];

/// Checks spritebox's `lint=allow(CODE,...)`, which has no effect in meshbox, as
/// spritebox checks it, so that a palette file valid here is valid there.
fn lint_option(token: Token) -> Parsed<()> {
    let invalid = || {
        token.error(
            ErrorCode::Syntax,
            format!("Expected `lint=allow(CODE,...)`, not `lint={}`", token.text),
        )
    };
    let inner = token
        .text
        .strip_prefix("allow(")
        .and_then(|t| t.strip_suffix(')'))
        .ok_or_else(invalid)?;
    let mut offset = "allow(".len();
    for part in inner.split(',') {
        let lead = part.chars().take_while(|c| *c == ' ').count();
        let name = part.trim();
        let pos = token.at(offset + lead);
        if name.is_empty() {
            return Err(Error::at(pos, ErrorCode::Syntax, "Expected a lint code"));
        }
        if !LINT_CODES.contains(&name) {
            return Err(Error::at(
                pos,
                ErrorCode::Syntax,
                format!(
                    "Unknown lint `{name}`; expected one of {}",
                    LINT_CODES.join(", ")
                ),
            ));
        }
        offset += part.chars().count() + 1;
    }
    Ok(())
}
