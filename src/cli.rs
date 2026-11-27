//! The command line: parsing arguments, reading files, writing outputs and reporting.

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
const EXIT_IO: u8 = 3;

struct Options {
    command: String,
    file: String,
    out: Option<PathBuf>,
    scale: u32,
    model: Option<String>,
    json: bool,
}

pub fn run(args: &[String]) -> ExitCode {
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") || args[0] == "help" {
        println!("{USAGE}");
        return if args.is_empty() {
            ExitCode::from(EXIT_USAGE)
        } else {
            ExitCode::SUCCESS
        };
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("meshbox {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let options = match parse_args(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("meshbox: {message}\n\n{USAGE}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    execute(&options)
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
        let mut value = |name: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg.as_str() {
            "--json" => json = true,
            "--out" if command == "render" => out = Some(PathBuf::from(value("--out")?)),
            "--scale" if command == "render" => {
                let text = value("--scale")?;
                let n: u32 = text
                    .parse()
                    .ok()
                    .filter(|n| (1..=32).contains(n))
                    .ok_or_else(|| format!("--scale is 1 to 32, not `{text}`"))?;
                scale = Some(n);
            }
            "--model" if command == "render" => model = Some(value("--model")?),
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
    let file = file.ok_or("missing <file>")?;
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

fn execute(options: &Options) -> ExitCode {
    let text = if options.file == "-" {
        let mut text = String::new();
        match std::io::stdin().read_to_string(&mut text) {
            Ok(_) => Ok(text),
            Err(e) => Err(e),
        }
    } else {
        std::fs::read_to_string(&options.file)
    };
    let text = match text {
        Ok(text) => text,
        Err(e) => {
            let error = Error::new(ErrorCode::Io, format!("cannot read {}: {e}", options.file));
            return report(options, &[error], &[], EXIT_IO);
        }
    };
    let base = base_dir(options);
    let mut loader = |name: &str| -> meshbox::Result<String> {
        let path = base.join(name);
        std::fs::read_to_string(&path)
            .map_err(|e| Error::new(ErrorCode::Io, format!("{}: {e}", path.display())))
    };
    let limits = Limits::default();
    let document = match Document::parse_with_loader(&text, &limits, &mut loader) {
        Ok(document) => document,
        Err(_) => {
            // All the errors, not only the first.
            let errors = meshbox::check_with_loader(&text, &limits, &mut loader);
            let status = if errors.iter().all(|e| e.code == ErrorCode::Io) {
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
    match render(options, &document, &limits) {
        Ok(written) => report(options, &[], &written, 0),
        Err((error, status)) => report(options, &[error], &[], status),
    }
}

/// Builds every output in memory, then writes them: nothing is written when any of
/// them fails.
fn render(
    options: &Options,
    document: &Document,
    limits: &Limits,
) -> Result<Vec<PathBuf>, (Error, u8)> {
    let invalid = |e: Error| (e, EXIT_INVALID);
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
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut models_json = Vec::new();
    for &name in &names {
        let mesh = document.mesh_with(name, limits).map_err(invalid)?;
        let glb = document.glb_with(name, limits).map_err(invalid)?;
        let file = format!("model-{name}.glb");
        let model = document.model(name).ok_or_else(|| {
            invalid(Error::new(
                ErrorCode::UnknownName,
                format!("Unknown model `{name}`"),
            ))
        })?;
        models_json.push(model_json(model, &file, &mesh));
        files.push((file, glb));
    }
    let texture = document.palette_texture_with(limits).map_err(invalid)?;
    let has_texture = texture.is_some();
    if let Some(texture) = texture {
        files.push(("palette.png".to_owned(), png::encode(texture.image())));
    }
    let preview_options = PreviewOptions {
        scale: options.scale,
        grid: true,
    };
    let preview = document
        .preview_models_with(&names, &preview_options, limits)
        .map_err(invalid)?;
    files.push(("preview.png".to_owned(), png::encode(&preview)));
    let mut json = String::from("{\n  \"format\": \"meshbox\",\n  \"version\": 1,\n");
    let _ = writeln!(
        json,
        "  \"generator\": \"meshbox {}\",",
        env!("CARGO_PKG_VERSION")
    );
    if has_texture {
        json.push_str("  \"palette\": \"palette.png\",\n");
    }
    json.push_str("  \"preview\": \"preview.png\",\n  \"models\": [");
    json.push_str(&models_json.join(","));
    if !models_json.is_empty() {
        json.push_str("\n  ");
    }
    json.push_str("]\n}\n");
    files.push(("meshbox.json".to_owned(), json.into_bytes()));

    let out = options.out.clone().unwrap_or_default();
    let io = |e: std::io::Error, path: &Path| {
        (
            Error::new(ErrorCode::Io, format!("{}: {e}", path.display())),
            EXIT_IO,
        )
    };
    std::fs::create_dir_all(&out).map_err(|e| io(e, &out))?;
    let mut written = Vec::new();
    for (name, bytes) in files {
        let path = out.join(name);
        std::fs::write(&path, bytes).map_err(|e| io(e, &path))?;
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
    let file = if options.file == "-" {
        "<stdin>"
    } else {
        options.file.as_str()
    };
    // An error in an imported file names it relative to the file given.
    let at = |e: &Error| -> String {
        match &e.file {
            Some(name) => {
                let path = base_dir(options).join(name);
                path.display().to_string()
            }
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
            let _ = write!(json, ",\"message\":{}}}", quote(&e.message));
        }
        json.push_str("],\"written\":[");
        for (i, path) in written.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&quote(&path.display().to_string()));
        }
        json.push_str("]}");
        println!("{json}");
    } else {
        for e in errors {
            match e.position {
                Some(p) => eprintln!(
                    "{}:{}:{}: error[{}]: {}",
                    at(e),
                    p.line,
                    p.column,
                    e.code,
                    e.message
                ),
                None => eprintln!("{}: error[{}]: {}", at(e), e.code, e.message),
            }
        }
        for path in written {
            println!("wrote {}", path.display());
        }
        if !errors.is_empty() {
            let n = errors.len();
            eprintln!("{file}: {n} error{}", if n == 1 { "" } else { "s" });
        } else if options.command == "check" {
            println!("{file}: ok");
        }
    }
    ExitCode::from(status)
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
