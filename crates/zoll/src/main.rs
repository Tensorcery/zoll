//! The `zoll` command.

use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{ColorChoice, Parser, Subcommand, ValueEnum};
use zoll_core::config;

/// A policy engine for files of any language: Rego policies over data formats,
/// code, and the code embedded in either - awk in a shell string, shell in a
/// YAML `run:`.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// This config instead of the one found from the working directory up.
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    /// When to color the output.
    #[arg(long, global = true, value_name = "WHEN", default_value_t = ColorChoice::Auto)]
    color: ColorChoice,

    #[command(subcommand)]
    command: Command,
}

// The arguments of the commands not built yet are parsed, and read by the
// stage that builds them (docs/plan.md).
#[expect(dead_code, reason = "read as each command is built")]
#[derive(Subcommand)]
enum Command {
    /// Check files against the policies.
    Check {
        /// Files and directories to check.
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// How violations are printed.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,

        /// Which results fail the run; overrides the config's `fail-on`.
        #[arg(long, value_parser = ["deny", "warn"])]
        fail_on: Option<String>,

        /// Fail when `zoll.lock` does not match the config's git policies.
        #[arg(long)]
        locked: bool,
    },
    /// Print the input a policy sees for a file and for each fragment in it.
    Inspect {
        /// The file.
        file: PathBuf,
    },
    /// Pin each git policy source to its commit in `zoll.lock`.
    Lock {
        /// Move every source to the commit its ref points at now.
        #[arg(long)]
        upgrade: bool,
    },
    /// List the languages zoll parses.
    Languages,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
    Github,
    Sarif,
}

/// Exit code 2: zoll itself failed - config, network, a policy that does
/// not load. 1 is for violations, once policies run.
const FAILED: u8 = 2;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut stderr = io::stderr().lock();
    let color = use_color(cli.color, stderr.is_terminal());
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let label = if color {
                "\x1b[1;31merror\x1b[0m"
            } else {
                "error"
            };
            // Nowhere is left to report a failed write to stderr.
            let _ = writeln!(stderr, "{label}: {message}");
            ExitCode::from(FAILED)
        }
    }
}

fn run(cli: &Cli) -> Result<(), String> {
    let name = match &cli.command {
        Command::Check { .. } => {
            let _config = load_config(cli.config.as_deref())?;
            return Ok(());
        }
        Command::Inspect { .. } => "inspect",
        Command::Lock { .. } => "lock",
        Command::Languages => "languages",
    };
    Err(format!("`zoll {name}` is not built yet"))
}

/// The config `--config` names, or the one found from the working directory up.
fn load_config(path: Option<&Path>) -> Result<config::Config, String> {
    let config = if let Some(path) = path {
        config::load(path)
    } else {
        let dir = env::current_dir().map_err(|e| e.to_string())?;
        config::discover(&dir)
    };
    config.map_err(|e| e.to_string())
}

/// Whether to color: `auto` colors a terminal unless `NO_COLOR` is set.
fn use_color(choice: ColorChoice, terminal: bool) -> bool {
    match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => terminal && env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
    }
}
