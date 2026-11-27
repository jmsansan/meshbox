//! The command line: parsing arguments, reading files, writing outputs and reporting.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use meshbox::{Document, Error, ErrorCode, Limits, PreviewOptions, png};

pub const USAGE: &str = "\
meshbox: low-poly and voxel 3D as plain text

Usage:
  meshbox check <file> [--json]
      Validate a file and print its diagnostics.
  meshbox render <file> --out <dir> [--scale <n>] [--model <name>] [--json]
      Validate a file, then write model-<name>.glb for each model (or only
      --model), palette.png, preview.png and meshbox.json to <dir>.

<file> may be `-` for standard input; its imports are then read relative to
the current directory.

Options:
  --json        Print the outcome as one JSON object on standard output.
  --out <dir>   Directory for output files; created if missing.
  --scale <n>   Pixels per voxel in the preview, 1..=32 (default 8).
  --model <n>   Write only this model's glb and preview.
  -h, --help    Print this help.
  -V, --version Print the version.

Exit status: 0 success, 1 the file has errors, 2 bad usage, 3 an I/O error.";

/// Exit statuses, stable for scripts and assistants.
const EXIT_INVALID: u8 = 1;
pub const EXIT_USAGE: u8 = 2;
pub const EXIT_IO: u8 = 3;

struct Options {
    command: String,
    file: String,
    out: Option<PathBuf>,
    scale: u32,
    model: Option<String>,
    json: bool,
}

pub fn run(args: &[String]) -> ExitCode {
    // `-h` asks for help, unless it is a value, as a model's name: `--model -h`.
    let help = has_flag(args.iter().map(|a| Some(a.as_str())), &["-h", "--help"]);
    if args.is_empty() || help || args[0] == "help" {
        return if !say(USAGE) {
            ExitCode::from(EXIT_IO)
        } else if args.is_empty() {
            ExitCode::from(EXIT_USAGE)
        } else {
            ExitCode::SUCCESS
        };
    }
    if args.len() == 1 && (args[0] == "-V" || args[0] == "--version") {
        return if say(&format!("meshbox {}", env!("CARGO_PKG_VERSION"))) {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(EXIT_IO)
        };
    }
    let options = match parse_args(args) {
        Ok(options) => options,
        Err(message) => {
            return usage_error(
                &message,
                has_flag(args.iter().map(|a| Some(a.as_str())), &["--json"]),
            );
        }
    };
    execute(&options)
}

/// Whether one of `flags` is among `args` as a flag, the way the arguments are parsed:
/// for `render`, `--model` takes any next argument, `--out` and `--scale` one that is
/// not an option; other commands take no values. `None` is an argument that is not
/// Unicode.
pub fn has_flag<'a>(args: impl IntoIterator<Item = Option<&'a str>>, flags: &[&str]) -> bool {
    let mut args = args.into_iter().peekable();
    let render = args.peek() == Some(&Some("render"));
    while let Some(arg) = args.next() {
        match arg {
            Some("--model") if render => {
                args.next();
            }
            Some("--out" | "--scale") if render => {
                if args
                    .peek()
                    .is_some_and(|next| !next.is_some_and(|n| n.starts_with("--")))
                {
                    args.next();
                }
            }
            Some(flag) if flags.contains(&flag) => return true,
            _ => {}
        }
    }
    false
}

/// Reports bad usage, as one JSON object with `json`, and returns status 2 (3 when
/// the report cannot be written).
pub fn usage_error(message: &str, json: bool) -> ExitCode {
    let printed = if json {
        say(&format!(
            "{{\"version\":1,\"ok\":false,\"diagnostics\":[{{\"code\":\"invalid_argument\",\"severity\":\"error\",\"message\":{}}}],\"written\":[]}}",
            quote(message)
        ))
    } else {
        warn(&format!("meshbox: {message}\n\n{USAGE}"))
    };
    ExitCode::from(if printed { EXIT_USAGE } else { EXIT_IO })
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut iter = args.iter();
    let command = iter.next().ok_or("missing command")?.clone();
    if command != "check" && command != "render" {
        return Err(format!("unknown command `{command}`"));
    }
    let mut file = None;
    let mut out = None;
    let mut scale = None;
    let mut model = None;
    let mut json = false;
    while let Some(arg) = iter.next() {
        // An option is not a value: `--out --json` is missing the directory.
        let mut value = |name: &str| {
            iter.next()
                .filter(|v| !v.starts_with("--"))
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--json" => json = true,
            "--out" if command == "render" => {
                let dir = value("--out")?;
                // An empty directory, as an unset variable gives, is not the current one.
                if dir.is_empty() {
                    return Err("--out needs a directory, not an empty value".to_owned());
                }
                out = Some(PathBuf::from(dir));
            }
            "--scale" if command == "render" => {
                let text = value("--scale")?;
                let n: u32 = text
                    .parse()
                    .ok()
                    .filter(|n| (1..=32).contains(n))
                    .ok_or_else(|| format!("--scale is 1 to 32, not `{text}`"))?;
                scale = Some(n);
            }
            // Any name may follow, `--draft` included.
            "--model" if command == "render" => {
                model = Some(iter.next().cloned().ok_or("--model needs a value")?);
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown option `{flag}` for `{command}`"));
            }
            path => {
                if file.is_some() {
                    return Err(format!("unexpected argument `{path}`"));
                }
                file = Some(path.to_owned());
            }
        }
    }
    let file = file
        .filter(|f| !f.is_empty())
        .ok_or("missing <file>, or an empty one")?;
    if command == "render" && out.is_none() {
        return Err("render needs --out <dir>".to_owned());
    }
    Ok(Options {
        command,
        file,
        out,
        scale: scale.unwrap_or(PreviewOptions::default().scale),
        model,
        json,
    })
}

/// A full path as an import name: `/` between parts, and on Windows without the
/// `\\?\` prefix that canonical paths have (`//server/share` for a network path).
fn import_name(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = if let Some(unc) = text.strip_prefix("//?/UNC/") {
        format!("//{unc}")
    } else if let Some(local) = text.strip_prefix("//?/") {
        local.to_owned()
    } else {
        text
    };
    // `.` and `..` resolved by name, as imports are, so that names compare equal.
    let (root, rest) = if let Some(rest) = text.strip_prefix("//") {
        ("//", rest)
    } else if let Some(rest) = text.strip_prefix('/') {
        ("/", rest)
    } else {
        ("", text.as_str())
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.len() > 1 || (root == "/" && !parts.is_empty()) => {
                parts.pop();
            }
            ".." => {}
            _ => parts.push(part),
        }
    }
    format!("{root}{}", parts.join("/"))
}

/// A file name of the parse as it is shown: relative to the file given when it is
/// beside or under it, as the author wrote it.
fn shown(options: &Options, name: &str) -> String {
    let base = base_dir(options);
    let dir = if base.as_os_str().is_empty() {
        Path::new(".")
    } else {
        base.as_path()
    };
    let Ok(dir) = std::path::absolute(dir) else {
        return name.to_owned();
    };
    let dir = format!("{}/", import_name(&dir));
    match name.strip_prefix(dir.as_str()) {
        Some(rest) => base.join(rest).display().to_string(),
        None => name.to_owned(),
    }
}

/// A message with the full names of files beside or under the file given shortened,
/// as in [`shown`].
fn shortened(options: &Options, message: &str) -> String {
    let base = base_dir(options);
    let dir = if base.as_os_str().is_empty() {
        Path::new(".")
    } else {
        base.as_path()
    };
    let Ok(dir) = std::path::absolute(dir) else {
        return message.to_owned();
    };
    let prefix = if base.as_os_str().is_empty() || options.file == "-" {
        String::new()
    } else {
        format!("{}/", base.to_string_lossy().replace('\\', "/"))
    };
    message.replace(&format!("`{}/", import_name(&dir)), &format!("`{prefix}"))
}

/// The directory that imports are read relative to.
fn base_dir(options: &Options) -> PathBuf {
    if options.file == "-" {
        return PathBuf::from(".");
    }
    Path::new(&options.file)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// Text from bytes, or a `syntax` error at the first byte that is not UTF-8.
fn utf8(bytes: Vec<u8>) -> meshbox::Result<String> {
    String::from_utf8(bytes).map_err(|e| {
        let bytes = e.as_bytes();
        let at = e.utf8_error().valid_up_to();
        let before = String::from_utf8_lossy(&bytes[..at]);
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        Error::at(
            meshbox::Position::new(line, column),
            ErrorCode::Syntax,
            "The text is not UTF-8",
        )
    })
}

/// Reads at most `max` bytes: `None` when there are more.
fn read_bounded(reader: impl std::io::Read, max: usize) -> std::io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    reader
        .take(u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok((bytes.len() <= max).then_some(bytes))
}

fn too_long(limit: usize) -> Error {
    Error::new(
        ErrorCode::LimitExceeded,
        format!("The text is more than the limit of {limit} bytes"),
    )
}

fn execute(options: &Options) -> ExitCode {
    let limits = Limits::default();
    let max = limits.max_text_bytes;
    let bytes = if options.file == "-" {
        read_bounded(std::io::stdin().lock(), max)
    } else {
        std::fs::File::open(&options.file).and_then(|f| read_bounded(f, max))
    };
    let text = match bytes {
        Ok(Some(bytes)) => match utf8(bytes) {
            Ok(text) => text,
            Err(error) => return report(options, &[error], &[], EXIT_INVALID),
        },
        Ok(None) => return report(options, &[too_long(max)], &[], EXIT_INVALID),
        Err(e) => {
            let error = Error::new(ErrorCode::Io, format!("cannot read {}: {e}", options.file));
            return report(options, &[error], &[], EXIT_IO);
        }
    };
    // The file given goes by its full path, so that every import resolves to a full
    // path too: a file reached by a relative and by an absolute spelling is one file.
    // Links are not followed, so imports resolve from where the file is named, as for
    // an imported file.
    let root_name = if options.file == "-" {
        std::path::absolute(".")
            .ok()
            .map(|dir| format!("{}/-", import_name(&dir)))
            .unwrap_or_default()
    } else {
        std::path::absolute(&options.file)
            .ok()
            .map(|path| import_name(&path))
            .unwrap_or_else(|| {
                Path::new(&options.file)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
    };
    let mut loader = FileLoader {
        base: base_dir(options),
        cache: HashMap::new(),
        files: HashMap::new(),
        root: None,
        limit: max,
        // What imports may add to the text: the rest of the limit.
        budget: max - text.len(),
        io_failed: false,
    };
    if options.file != "-" {
        if let Ok(path) = std::fs::canonicalize(&options.file) {
            loader.files.insert(path.clone(), root_name.clone());
            loader.root = Some(path);
        }
    }
    let parsed = {
        let mut load = |name: &str| loader.load(name);
        Document::parse_file_with_loader(&root_name, &text, &limits, &mut load)
    };
    let document = match parsed {
        Ok(document) => document,
        Err(first) => {
            // All the errors, not only the first; the files are the ones already read.
            let mut errors = {
                let mut load = |name: &str| loader.load(name);
                meshbox::check_file_with_loader(&root_name, &text, &limits, &mut load)
            };
            if errors.is_empty() {
                errors.push(first);
            }
            // A failed read is status 3 even when the diagnostics kept leave it out.
            let status = if loader.io_failed || errors.iter().any(|e| e.code == ErrorCode::Io) {
                EXIT_IO
            } else {
                EXIT_INVALID
            };
            return report(options, &errors, &[], status);
        }
    };
    if options.command == "check" {
        return report(options, &[], &[], 0);
    }
    let inputs: Vec<PathBuf> = loader.files.into_keys().collect();
    match render(options, &document, &limits, &inputs) {
        Ok(written) => report(options, &[], &written, 0),
        Err((error, status, written)) => report(options, &[error], &written, status),
    }
}

/// Reads imports from disk, each once: every pass of a run sees the same text.
struct FileLoader {
    base: PathBuf,
    /// The outcome of each name read.
    cache: HashMap<String, meshbox::Result<String>>,
    /// The name each file on disk was first read under.
    files: HashMap<PathBuf, String>,
    /// The file given, on disk.
    root: Option<PathBuf>,
    /// The limit of the text with its imports.
    limit: usize,
    /// The bytes that imports may still add, so that no more is ever held.
    budget: usize,
    /// Whether any read failed.
    io_failed: bool,
}

impl FileLoader {
    fn load(&mut self, name: &str) -> meshbox::Result<String> {
        if let Some(outcome) = self.cache.get(name) {
            return outcome.clone();
        }
        let outcome = self.read(name);
        if outcome.as_ref().is_err_and(|e| e.code == ErrorCode::Io) {
            self.io_failed = true;
        }
        self.cache.insert(name.to_owned(), outcome.clone());
        outcome
    }

    fn read(&mut self, name: &str) -> meshbox::Result<String> {
        let path = self.base.join(name);
        let io = |e: std::io::Error| Error::new(ErrorCode::Io, format!("{}: {e}", path.display()));
        // One file under two names, as `Art.meshbox` and `art.meshbox` on a file
        // system that ignores case, would be read and defined twice.
        let real = std::fs::canonicalize(&path).map_err(io)?;
        if let Some(first) = self.files.get(&real) {
            // The file given is always among the files importing this one.
            if self.root.as_ref() == Some(&real) {
                return Err(Error::new(
                    ErrorCode::Cycle,
                    format!("It is `{first}`, the file given, which imports itself"),
                ));
            }
            return Err(Error::new(
                ErrorCode::DuplicateName,
                format!("It is the file `{first}`, imported under another name"),
            ));
        }
        self.files.insert(real, name.to_owned());
        let file = std::fs::File::open(&path).map_err(io)?;
        let Some(bytes) = read_bounded(file, self.budget).map_err(io)? else {
            return Err(too_long(self.limit));
        };
        let text = utf8(bytes)?;
        self.budget -= text.len();
        Ok(text)
    }
}

/// Builds the preview, the palette and the manifest, which checks that everything
/// fits, then writes each model's glb as it is built, so that one is in memory at a
/// time. A failure while writing leaves the files written before it, which the
/// outcome lists.
fn render(
    options: &Options,
    document: &Document,
    limits: &Limits,
    inputs: &[PathBuf],
) -> Result<Vec<PathBuf>, (Error, u8, Vec<PathBuf>)> {
    let invalid = |e: Error| (e, EXIT_INVALID, Vec::new());
    let names: Vec<&str> = match &options.model {
        Some(name) => {
            if document.model(name).is_none() {
                return Err(invalid(Error::new(
                    ErrorCode::UnknownName,
                    format!("Unknown model `{name}`"),
                )));
            }
            vec![name.as_str()]
        }
        None => document.models().iter().map(|m| m.name()).collect(),
    };
    // On a file system that ignores case, two such names would write one file.
    let mut seen: HashMap<String, &str> = HashMap::new();
    for &name in &names {
        if let Some(other) = seen.insert(name.to_ascii_lowercase(), name) {
            return Err(invalid(Error::new(
                ErrorCode::DuplicateName,
                format!(
                    "Models `{other}` and `{name}` would write files whose names differ only in case"
                ),
            )));
        }
    }
    // One export of the models written: the vertex limit counts them together, and
    // the texture is made once.
    let export = document
        .export_models_with(&names, limits)
        .map_err(invalid)?;
    let preview_options = PreviewOptions {
        scale: options.scale,
        grid: true,
    };
    let preview = document
        .preview_models_with(&names, &preview_options, limits)
        .map_err(invalid)?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let palette = match export.palette_png() {
        Some(png) => png.to_vec(),
        // A document without colors still gets the file, one transparent texel.
        None => png::encode(&meshbox::Image::new(1, 1).map_err(invalid)?),
    };
    files.push(("palette.png".to_owned(), palette));
    files.push(("preview.png".to_owned(), png::encode(&preview)));
    drop(preview);
    let models_json: Vec<String> = export
        .meshes()
        .filter(|(n, _)| names.contains(n))
        .filter_map(|(name, mesh)| {
            let model = document.model(name)?;
            Some(model_json(model, &format!("model-{name}.glb"), mesh))
        })
        .collect();
    let mut json = String::from("{\n  \"format\": \"meshbox\",\n  \"version\": 1,\n");
    let _ = writeln!(
        json,
        "  \"generator\": \"meshbox {}\",",
        env!("CARGO_PKG_VERSION")
    );
    json.push_str("  \"palette\": \"palette.png\",\n");
    json.push_str("  \"preview\": \"preview.png\",\n  \"models\": [");
    json.push_str(&models_json.join(","));
    if !models_json.is_empty() {
        json.push_str("\n  ");
    }
    json.push_str("]\n}\n");
    files.push(("meshbox.json".to_owned(), json.into_bytes()));

    let out = options.out.clone().unwrap_or_default();
    let mut written = Vec::new();
    let io = |e: std::io::Error, path: &Path, written: &mut Vec<PathBuf>| {
        (
            Error::new(ErrorCode::Io, format!("{}: {e}", path.display())),
            EXIT_IO,
            std::mem::take(written),
        )
    };
    std::fs::create_dir_all(&out).map_err(|e| io(e, &out, &mut written))?;
    // An output must not replace the file given or one it imports.
    let outputs = export
        .meshes()
        .filter(|(n, _)| names.contains(n))
        .map(|(n, _)| format!("model-{n}.glb"))
        .chain(files.iter().map(|(n, _)| n.clone()));
    for name in outputs {
        let path = out.join(name);
        // Nor write through a link, which could lead to another output or an input.
        let link = std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
        let input = std::fs::canonicalize(&path).is_ok_and(|real| inputs.contains(&real));
        if link || input {
            let message = if link {
                format!(
                    "{} is a symbolic link; it is not written through",
                    path.display()
                )
            } else {
                format!("Writing {} would replace an input", path.display())
            };
            return Err((
                Error::new(ErrorCode::InvalidArgument, message),
                EXIT_USAGE,
                Vec::new(),
            ));
        }
    }
    for file in export.glb_files() {
        let (name, bytes) = match file {
            Ok(file) => file,
            Err(e) => return Err((e, EXIT_INVALID, written)),
        };
        if !names.iter().any(|n| name == format!("model-{n}.glb")) {
            continue;
        }
        let path = out.join(name);
        std::fs::write(&path, bytes).map_err(|e| io(e, &path, &mut written))?;
        written.push(path);
    }
    for (name, bytes) in files {
        let path = out.join(name);
        std::fs::write(&path, bytes).map_err(|e| io(e, &path, &mut written))?;
        written.push(path);
    }
    Ok(written)
}

fn model_json(model: &meshbox::VoxelModel, file: &str, mesh: &meshbox::Mesh) -> String {
    let stats = mesh.stats();
    let v3 = |v: [f32; 3]| format!("[{}, {}, {}]", v[0], v[1], v[2]);
    let mut out = String::new();
    let _ = write!(
        out,
        "\n    {{\n      \"name\": {},\n      \"file\": {},\n      \"size\": [{}, {}, {}],\n      \"origin\": {},\n      \"unit\": {},",
        quote(model.name()),
        quote(file),
        model.size()[0],
        model.size()[1],
        model.size()[2],
        v3(model.origin()),
        model.unit()
    );
    match mesh.bounds() {
        Some(b) => {
            let _ = write!(
                out,
                "\n      \"bounds\": {{\"min\": {}, \"max\": {}}},",
                v3(b.min),
                v3(b.max)
            );
        }
        None => out.push_str("\n      \"bounds\": null,"),
    }
    let _ = write!(
        out,
        "\n      \"triangles\": {},\n      \"vertices\": {}\n    }}",
        stats.triangles, stats.vertices
    );
    out
}

/// Prints errors and written files, and returns `status`.
fn report(options: &Options, errors: &[Error], written: &[PathBuf], status: u8) -> ExitCode {
    let mut printed = true;
    let file = if options.file == "-" {
        "<stdin>"
    } else {
        options.file.as_str()
    };
    // An error in an imported file names it relative to the file given.
    let at = |e: &Error| -> String {
        match &e.file {
            Some(name) => shown(options, name),
            None => file.to_owned(),
        }
    };
    if options.json {
        let mut json = String::new();
        let _ = write!(
            json,
            "{{\"version\":1,\"file\":{},\"ok\":{},\"diagnostics\":[",
            quote(file),
            status == 0
        );
        for (i, e) in errors.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            let _ = write!(
                json,
                "{{\"code\":{},\"severity\":\"error\"",
                quote(e.code.as_str())
            );
            if e.file.is_some() {
                let _ = write!(json, ",\"file\":{}", quote(&at(e)));
            }
            if let Some(p) = e.position {
                let _ = write!(json, ",\"line\":{},\"column\":{}", p.line, p.column);
            }
            let _ = write!(
                json,
                ",\"message\":{}}}",
                quote(&shortened(options, &e.message))
            );
        }
        json.push_str("],\"written\":[");
        for (i, path) in written.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&quote(&path.display().to_string()));
        }
        json.push_str("]}");
        printed &= say(&json);
    } else {
        for e in errors {
            match e.position {
                Some(p) => {
                    printed &= warn(&format!(
                        "{}:{}:{}: error[{}]: {}",
                        at(e),
                        p.line,
                        p.column,
                        e.code,
                        shortened(options, &e.message)
                    ));
                }
                None => {
                    printed &= warn(&format!(
                        "{}: error[{}]: {}",
                        at(e),
                        e.code,
                        shortened(options, &e.message)
                    ))
                }
            }
        }
        for path in written {
            printed &= say(&format!("wrote {}", path.display()));
        }
        if !errors.is_empty() {
            let n = errors.len();
            printed &= warn(&format!(
                "{file}: {n} error{}",
                if n == 1 { "" } else { "s" }
            ));
        } else if options.command == "check" {
            printed &= say(&format!("{file}: ok"));
        }
    }
    // Output that cannot be written is an I/O error.
    if printed {
        ExitCode::from(status)
    } else {
        ExitCode::from(EXIT_IO)
    }
}

/// A JSON string literal.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Prints a line on standard error; false when it cannot be written.
pub fn warn(text: &str) -> bool {
    use std::io::Write as _;
    let mut err = std::io::stderr().lock();
    writeln!(err, "{text}").and_then(|_| err.flush()).is_ok()
}

/// Prints a line on standard output; false when it cannot be written, as to a full
/// disk or a closed pipe.
fn say(text: &str) -> bool {
    use std::io::Write as _;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{text}").and_then(|_| out.flush()).is_ok()
}
