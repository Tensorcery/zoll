//! The `zoll` command.

use std::process::ExitCode;

use clap::Parser;

/// A policy engine for files of any language: Rego policies over data formats,
/// code, and the code embedded in either - awk in a shell string, shell in a
/// YAML `run:`.
#[derive(Parser)]
#[command(version)]
struct Cli {}

fn main() -> ExitCode {
    let _cli = Cli::parse();
    ExitCode::SUCCESS
}
