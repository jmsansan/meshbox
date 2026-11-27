//! The `meshbox` command: validate and render meshbox files.

use std::process::ExitCode;

const USAGE: &str = "\
meshbox: low-poly and voxel 3D as plain text

Usage:
  meshbox check <file> [--json]
  meshbox render <file> --out <dir> [--scale <n>] [--model <name>] [--json]

The commands arrive with the text format; this build only prints its version.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
