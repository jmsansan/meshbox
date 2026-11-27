//! `import NAME`: definitions of other files, loaded through the caller's loader.

use std::collections::{HashMap, HashSet};

use super::lex;
use crate::error::Errors;
use crate::{Error, ErrorCode, Limits, Result};

/// The longest name an import may resolve to.
const MAX_NAME_BYTES: usize = 1024;
/// The deepest imports go, whatever `max_nesting` says: loading recurses.
const MAX_DEPTH: usize = 256;

/// Loads the text of a file by the name an `import` resolves to.
pub(crate) type Loader<'l> = dyn FnMut(&str) -> Result<String> + 'l;

/// The files of a parse: the text parsed (file 0, named by the caller, or "") and every file it
/// imports, directly or not, each once.
pub(crate) struct Files {
    pub names: Vec<String>,
    pub texts: Vec<String>,
    /// The file that the `import` on `(file, line)` loads.
    pub targets: HashMap<(u32, usize), u32>,
    /// Each loaded file by name.
    index: HashMap<String, u32>,
    /// Names that could not be loaded: imported again, they are not read again.
    failed: HashSet<String>,
}

impl Files {
    /// Loads every file that `root` imports, reporting import errors.
    pub fn load(
        root: &str,
        root_name: &str,
        limits: &Limits,
        loader: Option<&mut Loader>,
        errors: &mut Errors,
    ) -> Files {
        // `./art/a.meshbox` is the name `art/a.meshbox` that its own imports resolve to.
        let root_name = resolve("", root_name);
        let mut files = Files {
            names: vec![root_name.clone()],
            texts: vec![root.to_owned()],
            targets: HashMap::new(),
            index: HashMap::from([(root_name.clone(), 0)]),
            failed: HashSet::new(),
        };
        let mut loading = Loading {
            limits,
            loader,
            errors,
            total: root.len(),
        };
        let mut stack = vec![0];
        files.visit(0, &mut stack, &mut loading);
        files
    }

    fn visit(&mut self, file: u32, stack: &mut Vec<u32>, loading: &mut Loading) {
        let text = self.texts[file as usize].clone();
        // Lexing errors are reported when the file is parsed.
        let nodes = lex::lex(
            &text,
            file,
            loading.limits.max_definitions,
            &mut Errors::default(),
        );
        for node in nodes.iter().filter(|n| n.keyword() == "import") {
            let tokens = &node.line.tokens;
            let Some(&name_token) = tokens.get(1) else {
                loading
                    .errors
                    .push(tokens[0].end_error_syntax("Expected the name of a file to import"));
                continue;
            };
            if let Some(extra) = tokens.get(2) {
                loading.errors.push(extra.error(
                    ErrorCode::Syntax,
                    format!("Unexpected `{}`; a file's name has no spaces", extra.text),
                ));
                continue;
            }
            if let Some(child) = node.children.first() {
                loading.errors.push(child.line.first().error(
                    ErrorCode::Syntax,
                    "Unexpected indented line under an import",
                ));
                continue;
            }
            // A name may be quoted, as in `import "palettes.meshbox"`.
            let written = name_token
                .text
                .strip_prefix('"')
                .and_then(|t| t.strip_suffix('"'))
                .unwrap_or(name_token.text);
            let name = resolve(&self.names[file as usize], written);
            let line = name_token.pos.line;
            if name.len() > MAX_NAME_BYTES {
                loading.errors.push(name_token.error(
                    ErrorCode::LimitExceeded,
                    format!("An imported file's name is at most {MAX_NAME_BYTES} bytes"),
                ));
                continue;
            }
            if self.failed.contains(&name) {
                continue;
            }
            if let Some(&i) = self.index.get(&name) {
                if stack.contains(&i) {
                    loading.errors.push(name_token.error(
                        ErrorCode::Cycle,
                        format!(
                            "`{name}` imports itself through `import {}`",
                            name_token.text
                        ),
                    ));
                } else {
                    self.targets.insert((file, line), i);
                }
                continue;
            }
            if stack.len() > loading.limits.max_nesting.min(MAX_DEPTH) {
                loading.errors.push(name_token.error(
                    ErrorCode::LimitExceeded,
                    format!(
                        "Imports nested more than the limit of {} deep",
                        loading.limits.max_nesting
                    ),
                ));
                continue;
            }
            let Some(loader) = loading.loader.as_mut() else {
                loading.errors.push(name_token.error(
                    ErrorCode::InvalidArgument,
                    format!(
                        "Cannot import `{name}`: imports need a loader, as `Document::parse_with_loader` takes"
                    ),
                ));
                continue;
            };
            match loader(&name) {
                Ok(text) => {
                    if loading.total.saturating_add(text.len()) > loading.limits.max_text_bytes {
                        loading.errors.push(name_token.error(
                            ErrorCode::LimitExceeded,
                            format!(
                                "With `{name}`, the text is more than the limit of {} bytes",
                                loading.limits.max_text_bytes
                            ),
                        ));
                        self.failed.insert(name);
                        continue;
                    }
                    loading.total += text.len();
                    let i = self.names.len() as u32;
                    self.index.insert(name.clone(), i);
                    self.names.push(name);
                    self.texts.push(text);
                    self.targets.insert((file, line), i);
                    stack.push(i);
                    self.visit(i, stack, loading);
                    stack.pop();
                }
                Err(e) => {
                    loading
                        .errors
                        .push(name_token.error(e.code, format!("Cannot import `{name}`: {e}")));
                    self.failed.insert(name);
                }
            }
        }
    }
}

struct Loading<'e, 'l, 'm> {
    limits: &'e Limits,
    loader: Option<&'m mut Loader<'l>>,
    errors: &'e mut Errors,
    total: usize,
}

impl lex::Token<'_> {
    fn end_error_syntax(&self, message: &str) -> Error {
        Error::at(self.end(), ErrorCode::Syntax, message)
    }
}

/// The name of `name` imported from the file `from`: relative to `from`'s directory
/// unless it starts with `/`, with `.` and `..` resolved, `/` between parts.
pub(crate) fn resolve(from: &str, name: &str) -> String {
    let joined = if name.starts_with('/') {
        name.to_owned()
    } else {
        match from.rfind('/') {
            Some(i) => format!("{}/{name}", &from[..i]),
            None => name.to_owned(),
        }
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|p| *p != "..") => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let path = parts.join("/");
    if joined.starts_with('/') {
        format!("/{path}")
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn names_resolve_from_the_importing_file() {
        assert_eq!(resolve("", "palettes.meshbox"), "palettes.meshbox");
        assert_eq!(resolve("", "./art/./a.meshbox"), "art/a.meshbox");
        assert_eq!(resolve("art/a.meshbox", "b.meshbox"), "art/b.meshbox");
        assert_eq!(resolve("art/a.meshbox", "../c.meshbox"), "c.meshbox");
        assert_eq!(resolve("a.meshbox", "../../c"), "../../c");
        assert_eq!(resolve("art/a", "/abs/x"), "/abs/x");
    }
}
