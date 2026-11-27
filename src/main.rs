//! The `meshbox` command: validate and render meshbox files.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod cli;

use std::process::ExitCode;

fn main() -> ExitCode {
    // Arguments that are not valid Unicode are refused rather than a panic.
    let args: Vec<String> = match std::env::args_os()
        .skip(1)
        .map(|a| a.into_string())
        .collect()
    {
        Ok(args) => args,
        Err(_) => {
            let raw: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
            let json = cli::has_flag(raw.iter().map(|a| a.to_str()), &["--json"]);
            return cli::usage_error("arguments must be valid Unicode", json);
        }
    };
    cli::run(&args)
}
