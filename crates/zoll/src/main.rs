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
use zoll_core::config::FailOn;
use zoll_core::git::{self, Mode};
use zoll_core::language::{self, LANGUAGES, Languages};
use zoll_core::policy::Policies;
use zoll_core::suppress::PerFile;
use zoll_core::{Error, SourceError, check, config, input, walk};

mod report;

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

        /// List the files that would be checked, each with its language, and
        /// check none.
        #[arg(long)]
        show_files: bool,
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

/// Exit code 1: violations that fail the run.
const VIOLATIONS: u8 = 1;

/// Exit code 2: zoll itself failed - config, network, a policy that does
/// not load.
const FAILED: u8 = 2;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let color = stream_color(cli.color);
    let mut stderr = AutoStream::new(io::stderr().lock(), color);
    let mut stdout = AutoStream::new(io::stdout().lock(), color);
    // Without a working directory, the paths given stay as they were given.
    let cwd = env::current_dir().unwrap_or_default();
    match run(&cli, &cwd, &mut stdout) {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(VIOLATIONS),
        // The reader stopped reading (`zoll inspect FILE | head`): nothing
        // went wrong.
        Err(Error::Io(_, error)) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            // Nowhere is left to report a failed write to stderr.
            let _ = writeln!(stderr, "{}", render(&error, &cwd));
            ExitCode::from(FAILED)
        }
    }
}

/// The command; `true` when violations fail the run.
fn run(cli: &Cli, cwd: &Path, out: &mut impl Write) -> Result<bool, Error> {
    match &cli.command {
        Command::Check {
            paths,
            show_files,
            fail_on,
            locked,
            format,
        } => {
            let loaded = load_config(cli.config.as_deref(), cwd)?;
            let languages = Languages::new(&loaded.config.languages)?;
            let stdin = paths.iter().any(|path| path.as_os_str() == "-");
            let paths: Vec<_> = paths
                .iter()
                .filter(|path| path.as_os_str() != "-")
                .map(|path| walk::clean(&cwd.join(path)))
                .collect();
            let mut files = walk::files(&paths, &loaded.root, &loaded.config, &languages)?;
            if stdin {
                // The standard input is YAML, as conftest takes it: what `helm
                // template` prints, piped in.
                files.push(walk::File {
                    path: PathBuf::from("-"),
                    relative: "-".to_owned(),
                    language: "yaml",
                });
            }
            if *show_files {
                for file in files {
                    writeln!(out, "{:<10} {}", file.language, shown(&file.path, cwd))
                        .map_err(stdout)?;
                }
                return Ok(false);
            }
            let mode = if *locked { Mode::Locked } else { Mode::Check };
            let git = git::resolve(&loaded.root, &loaded.config.policies, mode)?;
            let policies = Policies::load(&loaded.root, &loaded.config, &git.dirs)?;
            let per_file = PerFile::new(&loaded.config.per_file_ignores)?;
            let mut checked = check::check(files, &policies, &per_file)?;
            // Only a run over the whole project has seen every file a
            // `per-file-ignores` glob names.
            let whole = paths.iter().any(|path| loaded.root.starts_with(path));
            if whole && let Some(config) = &loaded.file {
                per_file.report_unused(config, &loaded.root, &mut checked)?;
            }
            let fail_on = match fail_on.as_deref() {
                Some("warn") => FailOn::Warn,
                Some(_) => FailOn::Deny,
                None => loaded.config.fail_on,
            };
            report::write(*format, &checked, cwd, fail_on, out).map_err(stdout)
        }
        Command::Inspect { file } => {
            let loaded = load_config(cli.config.as_deref(), cwd)?;
            let languages = Languages::new(&loaded.config.languages)?;
            let path = walk::clean(&cwd.join(file));
            std::fs::metadata(&path).map_err(|e| Error::Io(path.clone(), e))?;
            let relative = walk::relative(&path, &loaded.root);
            let Some(language) = languages.of(Path::new(&relative), &path) else {
                return Err(Error::Other(format!(
                    "{}: no language is known for this file; name one in the config's \
                     `languages` table",
                    shown(&path, cwd)
                )));
            };
            let file = walk::File {
                path,
                relative,
                language,
            };
            // What zoll notes is for `check` to report; `inspect` shows the input.
            let (evaluations, _) = input::evaluations(&file, &input::read(&file)?)?;
            serde_json::to_writer_pretty(&mut *out, &evaluations).map_err(|e| stdout(e.into()))?;
            writeln!(out).map_err(stdout)?;
            Ok(false)
        }
        Command::Languages => {
            for language in LANGUAGES.into_iter().filter(|l| language::compiled(l)) {
                writeln!(out, "{language}").map_err(stdout)?;
            }
            Ok(false)
        }
        Command::Lock { upgrade } => {
            let loaded = load_config(cli.config.as_deref(), cwd)?;
            let mode = if *upgrade { Mode::Upgrade } else { Mode::Lock };
            let git = git::resolve(&loaded.root, &loaded.config.policies, mode)?;
            for (source, commit) in &git.changed {
                writeln!(out, "{} at {}", git::redact(source), git::short(commit))
                    .map_err(stdout)?;
            }
            Ok(false)
        }
    }
}

fn stdout(error: io::Error) -> Error {
    Error::Io(PathBuf::from("stdout"), error)
}

/// The config `--config` names, or the one found from the working directory up.
fn load_config(path: Option<&Path>, cwd: &Path) -> Result<config::Loaded, Error> {
    match path {
        Some(path) => config::load(&walk::clean(&cwd.join(path))),
        None => config::discover(cwd),
    }
}

/// An error as rustc draws one: the message, then the line it is on,
/// marked. Styled always; `AutoStream` drops the style where it is not wanted.
fn render(error: &Error, cwd: &Path) -> String {
    let renderer = Renderer::styled();
    let title = |message: String| {
        renderer.render(&[Group::with_title(Level::ERROR.primary_title(message))])
    };
    let SourceError {
        path,
        text,
        message,
        span,
    } = match error {
        Error::Source(source) => source,
        Error::Io(path, error) => return title(format!("{}: {error}", shown(path, cwd))),
        Error::Other(message) => return title(message.clone()),
    };
    let path = shown(path, cwd);
    let Some(span) = span else {
        return title(format!("{path}: {message}"));
    };
    // "unknown field `x`, expected one of ..." - the first half heads the
    // error, the second marks the place.
    let (title, label) = match message.split_once(", expected ") {
        Some((title, expected)) => (title.to_owned(), Some(format!("expected {expected}"))),
        None => (message.clone(), None),
    };
    let snippet = Snippet::source(text.as_str())
        .path(path.as_str())
        .annotation(AnnotationKind::Primary.span(span.clone()).label(label));
    renderer.render(&[Level::ERROR.primary_title(title).element(snippet)])
}

/// A path as the user would type it: relative to the working directory when
/// it is under it.
pub(crate) fn shown(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd).unwrap_or(path).display().to_string()
}

fn stream_color(choice: ColorChoice) -> anstream::ColorChoice {
    match choice {
        ColorChoice::Auto => anstream::ColorChoice::Auto,
        ColorChoice::Always => anstream::ColorChoice::Always,
        ColorChoice::Never => anstream::ColorChoice::Never,
    }
}
