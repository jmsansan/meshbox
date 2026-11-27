use std::fmt;

/// Stable error categories. Match on the code rather than the message.
///
/// [`ErrorCode::as_str`] gives the name used in the command-line tool's JSON
/// diagnostics, which is stable too. The codes that spritebox also has mean the same
/// in both crates.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ErrorCode {
    /// Malformed text: an unknown directive, a missing value, bad indentation.
    Syntax,
    /// A value of the right shape but out of range, such as a color or a size.
    InvalidArgument,
    /// A reference to a palette, model or palette key that is not defined.
    UnknownName,
    /// A second definition of a name already defined.
    DuplicateName,
    /// A grid whose rows or columns do not match its declared size.
    SizeMismatch,
    /// Definitions or imports that refer to each other in a circle.
    Cycle,
    /// Input or output beyond the bounds of [`Limits`](crate::Limits).
    LimitExceeded,
    /// Output that cannot be encoded.
    EncodeError,
    /// A file could not be read or written.
    Io,
}

impl ErrorCode {
    /// Every code, in declaration order.
    pub const ALL: [ErrorCode; 9] = [
        ErrorCode::Syntax,
        ErrorCode::InvalidArgument,
        ErrorCode::UnknownName,
        ErrorCode::DuplicateName,
        ErrorCode::SizeMismatch,
        ErrorCode::Cycle,
        ErrorCode::LimitExceeded,
        ErrorCode::EncodeError,
        ErrorCode::Io,
    ];

    /// The code's stable name in `snake_case`, such as `"unknown_name"`.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Syntax => "syntax",
            ErrorCode::InvalidArgument => "invalid_argument",
            ErrorCode::UnknownName => "unknown_name",
            ErrorCode::DuplicateName => "duplicate_name",
            ErrorCode::SizeMismatch => "size_mismatch",
            ErrorCode::Cycle => "cycle",
            ErrorCode::LimitExceeded => "limit_exceeded",
            ErrorCode::EncodeError => "encode_error",
            ErrorCode::Io => "io",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A place in text: a 1-based line, and a 1-based column counted in characters.
///
/// Positions in an imported file say which file through [`Error::file`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Position {
    /// Which file of a parse: 0 for the text parsed, then imported files in the
    /// order loaded. First, so that errors sort by file, then line, then column.
    pub(crate) file: u32,
    /// The line, from 1.
    pub line: usize,
    /// The column, from 1, counted in characters rather than bytes.
    pub column: usize,
}

impl Position {
    /// A position in the text parsed itself, not in an imported file.
    pub fn new(line: usize, column: usize) -> Self {
        Self {
            file: 0,
            line,
            column,
        }
    }
    #[allow(dead_code)]
    pub(crate) fn in_file(file: u32, line: usize, column: usize) -> Self {
        Self { file, line, column }
    }
}

/// An error with a stable category, human-readable context and, for errors in text,
/// the position where it was found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    /// The error's category.
    pub code: ErrorCode,
    /// What is wrong, for people. Not stable: match on [`Error::code`].
    pub message: String,
    /// Where in the text the error is. Every parse and validation error has one;
    /// I/O and encoding errors do not.
    pub position: Option<Position>,
    /// The imported file that `position` is in, by the name its `import` resolves
    /// to; `None` for the text parsed itself.
    pub file: Option<String>,
}

impl Error {
    /// An error with no position, such as an I/O error.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            position: None,
            file: None,
        }
    }
    /// An error at `position` in text.
    pub fn at(position: Position, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            position: Some(position),
            file: None,
        }
    }
    #[allow(dead_code)]
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }
    pub(crate) fn limit(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::LimitExceeded, message)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(file) = &self.file {
            write!(f, "{file}, ")?;
        }
        match self.position {
            Some(p) => write!(f, "line {}, column {}: {}", p.line, p.column, self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::new(ErrorCode::Io, value.to_string())
    }
}

/// A result with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// The most errors one parse keeps: past them, more of the same file's mistakes add
/// nothing a person can act on, and keeping them would cost memory without bound.
pub(crate) const MAX_ERRORS: usize = 1000;

/// The errors of a parse: the first [`MAX_ERRORS`] by position, whatever order they
/// are found in, so that the first error of the text is always among them.
#[derive(Debug, Default)]
pub(crate) struct Errors {
    /// A max-heap by position and order found, so that the last is cheap to drop.
    heap: std::collections::BinaryHeap<Ranked>,
    found: u64,
}

#[derive(Debug)]
struct Ranked {
    key: (Option<Position>, u64),
    error: Error,
}

impl PartialEq for Ranked {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl Eq for Ranked {}
impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

impl Errors {
    pub(crate) fn push(&mut self, error: Error) {
        if !self.keeps(error.position) {
            return;
        }
        let key = (error.position, self.found);
        self.found += 1;
        self.heap.push(Ranked { key, error });
        if self.heap.len() > MAX_ERRORS {
            self.heap.pop();
        }
    }
    /// Whether an error at `position` would be kept: callers skip formatting the
    /// message of one that would not.
    pub(crate) fn keeps(&self, position: Option<Position>) -> bool {
        self.heap.len() < MAX_ERRORS || self.heap.peek().is_some_and(|last| position < last.key.0)
    }
    #[cfg(test)]
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Error> {
        let mut all: Vec<&Ranked> = self.heap.iter().collect();
        all.sort();
        all.into_iter().map(|r| &r.error)
    }
    /// The errors, ordered by position and, at one position, in the order found.
    pub(crate) fn into_vec(self) -> Vec<Error> {
        self.heap
            .into_sorted_vec()
            .into_iter()
            .map(|r| r.error)
            .collect()
    }
}
