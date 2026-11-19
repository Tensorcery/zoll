//! The `zoll` command.

use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use annotate_snippets::{AnnotationKind, Group, Level, Renderer, Snippet};
use anstream::AutoStream;
use anstyle::{AnsiColor, Effects};
use clap::builder::Styles;
use clap::{ColorChoice, Parser, Subcommand, ValueEnum};
use zoll_core::config;

/// The colors of `--help`, as cargo and uv have them.
const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .valid(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .invalid(AnsiColor::Yellow.on_default().effects(Effects::BOLD));

/// A policy engine for files of any language: Rego policies over data formats,
/// code, and the code embedded in either - awk in a shell string, shell in a
/// YAML `run:`.
#[derive(Parser)]
#[command(version, styles = STYLES)]
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
    let mut stderr = AutoStream::new(io::stderr().lock(), stream_color(cli.color));
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            // Nowhere is left to report a failed write to stderr.
            let _ = writeln!(stderr, "{}", render(&failure));
            ExitCode::from(FAILED)
        }
    }
}

/// Why zoll itself failed.
enum Failure {
    Config(config::Error),
    Other(String),
}

fn run(cli: &Cli) -> Result<(), Failure> {
    let name = match &cli.command {
        Command::Check { .. } => {
            let _config = load_config(cli.config.as_deref())?;
            return Ok(());
        }
        Command::Inspect { .. } => "inspect",
        Command::Lock { .. } => "lock",
        Command::Languages => "languages",
    };
    Err(Failure::Other(format!("`zoll {name}` is not built yet")))
}

/// The config `--config` names, or the one found from the working directory up.
fn load_config(path: Option<&Path>) -> Result<config::Config, Failure> {
    let config = if let Some(path) = path {
        config::load(path)
    } else {
        let dir = env::current_dir().map_err(|e| Failure::Other(e.to_string()))?;
        config::discover(&dir)
    };
    config.map_err(Failure::Config)
}

/// A failure as rustc draws an error: the message, then the line it is on,
/// marked. Styled always; `AutoStream` drops the style where it is not wanted.
fn render(failure: &Failure) -> String {
    let renderer = Renderer::styled();
    let Failure::Config(config::Error::Parse {
        path,
        text,
        message,
        span: Some(span),
    }) = failure
    else {
        let message = match failure {
            Failure::Config(error) => error.to_string(),
            Failure::Other(message) => message.clone(),
        };
        return renderer.render(&[Group::with_title(Level::ERROR.primary_title(message))]);
    };
    // "unknown field `x`, expected one of ..." - the first half heads the
    // error, the second marks the place.
    let (title, label) = match message.split_once(", expected ") {
        Some((title, expected)) => (title.to_owned(), Some(format!("expected {expected}"))),
        None => (message.clone(), None),
    };
    let path = shown(path);
    let snippet = Snippet::source(text.as_str())
        .path(path.as_str())
        .annotation(AnnotationKind::Primary.span(span.clone()).label(label));
    renderer.render(&[Level::ERROR.primary_title(title).element(snippet)])
}

/// A path as the user would type it: relative to the working directory when
/// it is under it.
fn shown(path: &Path) -> String {
    env::current_dir()
        .ok()
        .and_then(|dir| path.strip_prefix(dir).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}

fn stream_color(choice: ColorChoice) -> anstream::ColorChoice {
    match choice {
        ColorChoice::Auto => anstream::ColorChoice::Auto,
        ColorChoice::Always => anstream::ColorChoice::Always,
        ColorChoice::Never => anstream::ColorChoice::Never,
    }
}
