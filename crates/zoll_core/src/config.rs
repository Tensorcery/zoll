//! The configuration: where it is found and what it may hold
//! (docs/design.md, section 9).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use std::fmt;

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use toml::Spanned;

use crate::error::{Error, SourceError};

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
    /// Policy sources: a directory, or `git+<url>//<dir>@<ref>`; `policy/`
    /// when the config names none, as conftest's default.
    #[serde(default = "policy_dir")]
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
    /// A file name or glob, bound to its language; in the order written,
    /// as the first glob that matches wins.
    #[serde(default, deserialize_with = "in_order")]
    pub languages: Vec<(String, String)>,
    /// A glob, bound to the rule ids not reported in the files it matches.
    #[serde(default)]
    pub per_file_ignores: BTreeMap<String, Vec<Spanned<String>>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            include: everything(),
            exclude: Vec::new(),
            policies: policy_dir(),
            data: Vec::new(),
            fail_on: FailOn::default(),
            rego_version: RegoVersion::default(),
            packages: BTreeMap::new(),
            languages: Vec::new(),
            per_file_ignores: BTreeMap::new(),
        }
    }
}

/// A table's entries in the order written, which a map would sort.
fn in_order<'de, D: Deserializer<'de>>(table: D) -> Result<Vec<(String, String)>, D::Error> {
    struct Entries;
    impl<'de> Visitor<'de> for Entries {
        type Value = Vec<(String, String)>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a table")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::new();
            while let Some(entry) = map.next_entry()? {
                entries.push(entry);
            }
            Ok(entries)
        }
    }
    table.deserialize_map(Entries)
}

fn policy_dir() -> Vec<String> {
    vec!["policy/".to_owned()]
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

/// A config, and the directory its globs and paths are relative to.
#[derive(Debug)]
pub struct Loaded {
    /// The settings.
    pub config: Config,
    /// The directory of the config file; the working directory when there
    /// is none.
    pub root: PathBuf,
    /// The config file; `None` when there is none.
    pub file: Option<PathBuf>,
}

/// The config of a directory: the first config file in it or in a directory
/// above; the defaults when there is none.
///
/// # Errors
///
/// The config found is unreadable or invalid.
pub fn discover(dir: &Path) -> Result<Loaded, Error> {
    for root in dir.ancestors() {
        for name in NAMES {
            let path = root.join(name);
            if !path.is_file() {
                continue;
            }
            if let Some(config) = parse(&path)? {
                let root = root.to_owned();
                return Ok(Loaded {
                    config,
                    root,
                    file: Some(path),
                });
            }
        }
    }
    let root = dir.to_owned();
    Ok(Loaded {
        config: Config::default(),
        root,
        file: None,
    })
}

/// The config in this file, named by `--config`; its paths are relative to
/// the file's directory, as a found config's are.
///
/// # Errors
///
/// The file is unreadable or invalid. A `pyproject.toml` without
/// `[tool.zoll]` gives the defaults.
pub fn load(path: &Path) -> Result<Loaded, Error> {
    let config = parse(path)?.unwrap_or_default();
    let root = path.parent().unwrap_or(path).to_owned();
    Ok(Loaded {
        config,
        root,
        file: Some(path.to_owned()),
    })
}

/// The config in a file; `None` for a `pyproject.toml` that has no
/// `[tool.zoll]`.
fn parse(path: &Path) -> Result<Option<Config>, Error> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::Io(path.to_owned(), e))?;
    let parse_error = |e: toml::de::Error| {
        Error::Source(SourceError {
            path: path.to_owned(),
            text: text.clone(),
            // serde's words for the TOML ones.
            message: e
                .message()
                .replace("a sequence", "an array")
                .replace("a map", "a table"),
            span: e.span(),
        })
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
