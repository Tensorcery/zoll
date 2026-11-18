//! The `zoll` command.

use std::process::ExitCode;

use clap::Parser;

/// Finds code hidden inside other code and data - awk in a shell string, shell
/// in a YAML `run:` - and judges it by Rego policies.
#[derive(Parser)]
#[command(version)]
struct Cli {}

fn main() -> ExitCode {
    let _cli = Cli::parse();
    ExitCode::SUCCESS
}
