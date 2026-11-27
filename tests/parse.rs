use std::time::{Duration, Instant};

use meshbox::{Document, ErrorCode, check};

fn codes(text: &str) -> Vec<(usize, usize, ErrorCode)> {
    check(text)
        .iter()
        .map(|e| {
            let p = e.position.unwrap();
            (p.line, p.column, e.code)
        })
        .collect()
}

const VALID: &str = "\
; A comment line, then a blank line.

palette p   ; a comment after a definition
  a #ff0000
  b #00ff0080

voxels cube 2x2x2 palette=p origin=1,0,1 unit=0.5
  level 0
    ab
    ba
  level 1
    a.
    .b
";

#[test]
fn a_valid_file_has_no_errors() {
    assert!(check(VALID).is_empty(), "{:?}", check(VALID));
    let doc = Document::parse(VALID).unwrap();
    assert_eq!(doc.models().len(), 1);
    assert_eq!(doc.palettes().len(), 1);
    // CRLF line ends read the same.
    let crlf = VALID.replace('\n', "\r\n");
    assert_eq!(Document::parse(&crlf).unwrap(), doc);
}

#[test]
fn definitions_start_at_column_one() {
    assert_eq!(codes(" palette p\n"), [(1, 2, ErrorCode::Syntax)]);
}

#[test]
fn blocks_keep_one_indentation() {
    assert_eq!(
        codes("palette p\n  a #ff0000\n   b #00ff00\n"),
        [(3, 4, ErrorCode::Syntax)]
    );
}

#[test]
fn tabs_are_an_error() {
    assert_eq!(
        codes("palette p\n\ta #ff0000\n"),
        [(2, 1, ErrorCode::Syntax)]
    );
}

#[test]
fn names_use_ascii_letters_digits_underscore_and_dash() {
    assert!(check("palette a_b-C9\n  a #ffffff\n").is_empty());
    assert_eq!(codes("palette a.b\n"), [(1, 9, ErrorCode::Syntax)]);
    assert_eq!(codes("palette é\n"), [(1, 9, ErrorCode::Syntax)]);
    assert_eq!(codes("palette\n"), [(1, 8, ErrorCode::Syntax)]);
}

#[test]
fn columns_count_characters_not_bytes() {
    // `é` is two bytes in UTF-8 and one column.
    assert_eq!(
        codes("palette p\n  é #ff0000\n\nvoxels m 2x1x1 palette=p\n  level 0\n    éz\n"),
        [(6, 6, ErrorCode::UnknownName)]
    );
}

#[test]
fn unknown_directives() {
    let errors = check("mesh m\n");
    assert_eq!(errors[0].code, ErrorCode::Syntax);
    assert!(errors[0].message.contains("Unknown directive `mesh`"));
}

#[test]
fn options_are_given_once() {
    assert_eq!(
        codes("palette p\n  a #ffffff\nvoxels m 1x1x1 palette=p palette=p\n"),
        [(3, 26, ErrorCode::Syntax)]
    );
}

#[test]
fn check_reports_every_error_in_order_and_parse_the_first() {
    let text = "palette p\n  a #ff00\n\nvoxels m 1x1x1 palette=q\n\nvoxels n 2x1x1 palette=p\n  level 0\n    a\n";
    let errors = check(text);
    let found: Vec<(usize, usize, ErrorCode)> = errors
        .iter()
        .map(|e| (e.position.unwrap().line, e.position.unwrap().column, e.code))
        .collect();
    assert_eq!(
        found,
        [
            (2, 5, ErrorCode::InvalidArgument),
            (4, 24, ErrorCode::UnknownName),
            (8, 6, ErrorCode::SizeMismatch),
        ]
    );
    assert_eq!(Document::parse(text).unwrap_err(), errors[0]);
    // Every code has a stable JSON name.
    assert_eq!(errors[1].code.as_str(), "unknown_name");
}

#[test]
fn errors_display_their_position() {
    let error = Document::parse("palette p\n  a #ff00\n").unwrap_err();
    assert_eq!(
        error.to_string(),
        "line 2, column 5: Color `#ff00` must be `#rrggbb`, `#rrggbbaa` or `transparent`"
    );
}

/// xorshift, so the inputs are the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[test]
fn malformed_input_never_panics() {
    let start = Instant::now();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let alphabet: Vec<char> =
        "palette voxels level from= palette= origin= unit= 0123456789x,.-/#abcdefr ;\n\n  \t=()é"
            .chars()
            .collect();
    let valid: Vec<char> = VALID.chars().collect();
    for i in 0..10_000 {
        let text: String = if i % 2 == 0 {
            // Random text of the format's words and characters.
            let len = rng.below(200);
            (0..len)
                .map(|_| alphabet[rng.below(alphabet.len())])
                .collect()
        } else {
            // A valid file with a few characters changed, inserted or removed.
            let mut chars = valid.clone();
            for _ in 0..1 + rng.below(4) {
                let at = rng.below(chars.len());
                match rng.below(3) {
                    0 => chars[at] = alphabet[rng.below(alphabet.len())],
                    1 => chars.insert(at, alphabet[rng.below(alphabet.len())]),
                    _ => {
                        chars.remove(at);
                    }
                }
            }
            chars.into_iter().collect()
        };
        let errors = check(&text);
        for e in &errors {
            let p = e.position.expect("every parse error has a position");
            assert!(p.line >= 1 && p.column >= 1);
        }
        // Errors are ordered by position.
        assert!(errors.windows(2).all(|w| w[0].position <= w[1].position));
        if let Ok(doc) = Document::parse(&text) {
            let _ = doc.palette_texture();
        }
    }
    // Random bytes, through the lossy UTF-8 a caller would use.
    for _ in 0..10_000 {
        let bytes: Vec<u8> = (0..rng.below(300)).map(|_| rng.next() as u8).collect();
        let _ = check(&String::from_utf8_lossy(&bytes));
    }
    assert!(
        start.elapsed() < Duration::from_secs(30),
        "{:?}",
        start.elapsed()
    );
}
