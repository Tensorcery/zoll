//! The configuration: where it is found and what it may hold
//! (docs/design.md, section 9).

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The files a directory may hold the config in, in the order they are
/// tried, as ruff tries its own.
const NAMES: [&str; 3] = [".zoll.toml", "zoll.toml", "pyproject.toml"];

/// Every setting, with its default when the config leaves it out.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// Globs of the files to check.
    #[serde(default = "everything")]
    pub include: Vec<String>,
    /// Globs of the files to skip, on top of `.gitignore`.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Policy sources: a directory, or `git+<url>//<dir>@<ref>`.
    #[serde(default)]
    pub policies: Vec<String>,
    /// Directories of JSON and YAML loaded under `data.*`.
    #[serde(default)]
    pub data: Vec<PathBuf>,
    /// Which results fail the run.
    #[serde(default)]
    pub fail_on: FailOn,
    /// The Rego syntax the policies are written in.
    #[serde(default)]
    pub rego_version: RegoVersion,
    /// A package that is not a language name, bound to the language it runs on.
    #[serde(default)]
    pub packages: BTreeMap<String, String>,
    /// A file name or glob, bound to its language.
    #[serde(default)]
    pub languages: BTreeMap<String, String>,
    /// A glob, bound to the rule ids not reported in the files it matches.
    #[serde(default)]
    pub per_file_ignores: BTreeMap<String, Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            include: everything(),
            exclude: Vec::new(),
            policies: Vec::new(),
            data: Vec::new(),
            fail_on: FailOn::default(),
            rego_version: RegoVersion::default(),
            packages: BTreeMap::new(),
            languages: BTreeMap::new(),
            per_file_ignores: BTreeMap::new(),
        }
    }
}

fn everything() -> Vec<String> {
    vec!["**/*".to_owned()]
}

/// Which results fail the run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailOn {
    /// `deny` and `violation` results fail; `warn` results are only shown.
    #[default]
    Deny,
    /// `warn` results fail too.
    Warn,
}

/// The Rego syntax the policies are written in.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RegoVersion {
    /// Rego before `if` and `contains` were required.
    V0,
    /// Today's Rego, the default of conftest and regorus.
    #[default]
    V1,
}

/// A config that could not be read.
#[derive(Debug)]
pub enum Error {
    /// The file is missing or unreadable.
    Read(PathBuf, io::Error),
    /// The file is not TOML, or holds a field or a value zoll does not take.
    Parse {
        /// The file.
        path: PathBuf,
        /// Its text, for the command to show the line.
        text: String,
        /// What is wrong, and what was expected.
        message: String,
        /// Where in `text`, in bytes, when the parser knows.
        span: Option<Range<usize>>,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(path, error) => write!(f, "{}: {error}", path.display()),
            Self::Parse { path, message, .. } => write!(f, "{}: {message}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

/// The config of a directory: the first config file in it or in a directory
/// above; the defaults when there is none.
///
/// # Errors
///
/// The config found is unreadable or invalid.
pub fn discover(dir: &Path) -> Result<Config, Error> {
    for dir in dir.ancestors() {
        for name in NAMES {
            let path = dir.join(name);
            if !path.is_file() {
                continue;
            }
            if let Some(config) = parse(&path)? {
                return Ok(config);
            }
        }
    }
    Ok(Config::default())
}

/// The config in this file, named by `--config`.
///
/// # Errors
///
/// The file is unreadable or invalid. A `pyproject.toml` without
/// `[tool.zoll]` gives the defaults.
pub fn load(path: &Path) -> Result<Config, Error> {
    Ok(parse(path)?.unwrap_or_default())
}

/// The config in a file; `None` for a `pyproject.toml` that has no
/// `[tool.zoll]`.
fn parse(path: &Path) -> Result<Option<Config>, Error> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::Read(path.to_owned(), e))?;
    let parse_error = |e: toml::de::Error| Error::Parse {
        path: path.to_owned(),
        text: text.clone(),
        // serde's words for the TOML ones.
        message: e
            .message()
            .replace("a sequence", "an array")
            .replace("a map", "a table"),
        span: e.span(),
    };
    if path
        .file_name()
        .is_some_and(|name| name == "pyproject.toml")
    {
        let pyproject: Pyproject = toml::from_str(&text).map_err(parse_error)?;
        Ok(pyproject.tool.and_then(|tool| tool.zoll))
    } else {
        toml::from_str(&text).map(Some).map_err(parse_error)
    }
}

/// The part of `pyproject.toml` that is zoll's; the rest is someone else's.
#[derive(Deserialize)]
struct Pyproject {
    tool: Option<Tool>,
}

#[derive(Deserialize)]
struct Tool {
    zoll: Option<Config>,
}
