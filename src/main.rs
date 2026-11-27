//! The `meshbox` command: validate and render meshbox files.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::process::ExitCode;

const USAGE: &str = "\
meshbox: low-poly and voxel 3D as plain text

Usage:
  meshbox check <file> [--json]
  meshbox render <file> --out <dir> [--scale <n>] [--model <name>] [--json]

The commands arrive with the text format; this build only prints its version.";

fn main() -> ExitCode {
    // Arguments that are not valid Unicode are refused rather than a panic.
    let args: Vec<String> = match std::env::args_os()
        .skip(1)
        .map(|a| a.into_string())
        .collect()
    {
        Ok(args) => args,
        Err(_) => {
            eprintln!("meshbox: arguments must be valid Unicode\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("meshbox {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    println!("{USAGE}");
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}
