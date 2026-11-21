//! The policies: loaded once, bound to the languages they run on, evaluated
//! on each document as conftest evaluates them (docs/design.md, 8.4-8.6).

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use regorus::Engine;
use regorus::unstable::{Rule, RuleHead};
use serde_json::{Map, Value};

use crate::config::{Config, RegoVersion};
use crate::document::{self, Place, Step};
use crate::error::{Error, SourceError, line_column_span};
use crate::input::Evaluation;
use crate::language::LANGUAGES;
use crate::walk::{File, relative};

pub use crate::input::Severity;

/// What a rule found, before it is placed in the file.
#[derive(Debug, Clone)]
pub struct Violation {
    /// `package.suffix`: `bash.curl_pipe` (section 8.6).
    pub rule: String,
    /// Its weight.
    pub severity: Severity,
    /// The message.
    pub message: String,
    /// What the result points at, when it names something.
    pub at: Option<At>,
    /// The line conftest's `_loc` names, when it does and `at` does not.
    pub line: Option<usize>,
    /// The result's other keys, as conftest keeps them.
    pub metadata: Map<String, Value>,
}

/// What a result's `at` points at (docs/design.md, section 8.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum At {
    /// A path in a data document.
    Path(Vec<Step>),
    /// A node of a code tree: its `start` and `end`, each a line and a
    /// column, both from 1.
    Node {
        /// Where it starts.
        start: (usize, usize),
        /// Where it ends, past its last character.
        end: (usize, usize),
    },
}

/// A file with what was found in it.
#[derive(Debug)]
pub struct Checked {
    /// The file.
    pub file: File,
    /// Its text, for the report to show the lines.
    pub text: String,
    /// What was found, in the order of the file.
    pub found: Vec<Found>,
}

/// A violation, and where it is.
#[derive(Debug)]
pub struct Found {
    /// What the rule found.
    pub violation: Violation,
    /// Where in the file.
    pub place: Place,
    /// The fragments it was found through, the outermost first: each
    /// `{"language", "at"}`, `at` where it sits in its host (section 8.3).
    /// Empty for a result on the file itself.
    pub fragments: Vec<Value>,
}

impl At {
    fn from_json(value: &Value) -> Option<Self> {
        let position = |key| {
            let place = value.get(key)?;
            let number = |key| usize::try_from(place.get(key)?.as_u64()?).ok();
            Some((number("line")?, number("column")?))
        };
        match value {
            Value::Array(steps) => steps
                .iter()
                .map(Step::from_json)
                .collect::<Option<_>>()
                .map(Self::Path),
            Value::Object(_) => Some(Self::Node {
                start: position("start")?,
                end: position("end")?,
            }),
            _ => None,
        }
    }
}

/// The policies of a run, compiled once.
#[derive(Debug)]
pub struct Policies {
    engine: Engine,
    /// The packages that run on each language.
    runs: HashMap<&'static str, Vec<Package>>,
}

#[derive(Debug)]
struct Package {
    name: String,
    /// The rules conftest would run, by name, with their weight.
    rules: Vec<(String, Severity)>,
    has_exception: bool,
}

impl Policies {
    /// The config's policies and data, from its directory; a git source
    /// from its directory in `git`, fetched before.
    ///
    /// # Errors
    ///
    /// A source that is not there or holds no policy, a policy that does
    /// not compile, `package main` unbound, data that does not parse.
    pub fn load(
        root: &Path,
        config: &Config,
        git: &HashMap<String, PathBuf>,
    ) -> Result<Self, Error> {
        let mut engine = Engine::new();
        engine.set_rego_v0(config.rego_version == RegoVersion::V0);
        let mut loaded = 0;
        for source in &config.policies {
            let dir = match git.get(source) {
                Some(dir) => dir.clone(),
                None if source.starts_with("git+") => {
                    return Err(Error::Other(format!("`{source}` was not fetched")));
                }
                None => PathBuf::from(source),
            };
            for path in files(root, dir, &["rego"], "policies")? {
                let text =
                    std::fs::read_to_string(&path).map_err(|e| Error::Io(path.clone(), e))?;
                engine
                    .add_policy(relative(&path, root), text.clone())
                    .map_err(|e| rego_error(&e.to_string(), path.clone(), text))?;
                loaded += 1;
            }
        }
        if config.policies.is_empty() {
            return Err(Error::Other(
                "the config's `policies` is empty; name a directory of policies, or a git source"
                    .to_owned(),
            ));
        }
        if loaded == 0 {
            return Err(Error::Other(format!(
                "no policies in {}; the config's `policies` names where they are",
                config.policies.join(", ")
            )));
        }
        engine
            .add_data(regorus::Value::from(data(root, &config.data)?))
            .map_err(|e| Error::Other(e.to_string()))?;
        let runs = packages(&mut engine, &config.packages)?;
        Ok(Self { engine, runs })
    }

    /// What the policies of `language` find in one document.
    ///
    /// # Errors
    ///
    /// A rule that fails to evaluate, or yields what conftest would refuse.
    pub fn check(&self, language: &str, evaluation: &Evaluation) -> Result<Vec<Violation>, Error> {
        let Some(packages) = self.runs.get(language) else {
            return Ok(Vec::new());
        };
        let mut engine = self.engine.clone();
        engine.set_input(regorus::Value::from(evaluation.input.clone()));
        engine
            .add_data(regorus::Value::from(evaluation.data.clone()))
            .map_err(|e| Error::Other(e.to_string()))?;
        let mut found = Vec::new();
        for package in packages {
            let excepted = if package.has_exception {
                exceptions(&mut engine, &package.name)?
            } else {
                BTreeSet::new()
            };
            for (rule, severity) in &package.rules {
                let suffix = suffix(rule);
                if excepted.contains(suffix) {
                    continue;
                }
                let id = if suffix.is_empty() {
                    package.name.clone()
                } else {
                    format!("{}.{suffix}", package.name)
                };
                for result in results(&mut engine, &package.name, rule)? {
                    found.push(violation(
                        result,
                        id.clone(),
                        *severity,
                        &package.name,
                        rule,
                    )?);
                }
            }
        }
        Ok(found)
    }
}

/// regorus's error for a policy that does not compile, which reads
/// `--> path:line:col`, the line, then `error: what`, as a file's error.
fn rego_error(message: &str, path: PathBuf, text: String) -> Error {
    let place = message
        .lines()
        .find_map(|line| line.trim().strip_prefix("--> "))
        .and_then(|place| {
            let mut parts = place.rsplitn(3, ':');
            let column = parts.next()?.parse().ok()?;
            let line = parts.next()?.parse().ok()?;
            Some((line, column))
        });
    let what = message
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("error: "))
        .unwrap_or(message.trim());
    let Some((line, column)) = place else {
        return Error::Other(format!("{}: {}", path.display(), message.trim()));
    };
    Error::Source(SourceError {
        span: Some(line_column_span(&text, line, column)),
        message: what.to_owned(),
        path,
        text,
    })
}

/// The files under `source` (or `source` itself), from `root`, with one of
/// these extensions, in path order.
fn files(
    root: &Path,
    source: impl AsRef<Path>,
    extensions: &[&str],
    what: &str,
) -> Result<Vec<PathBuf>, Error> {
    let source = source.as_ref();
    let path = root.join(source);
    if !path.exists() {
        return Err(Error::Other(format!(
            "no {what}: `{}` is not there; the config's `{what}` names where they are",
            source.display()
        )));
    }
    let mut found: Vec<PathBuf> = ignore::WalkBuilder::new(path)
        .standard_filters(false)
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .map(ignore::DirEntry::into_path)
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| extensions.contains(&e))
        })
        .collect();
    found.sort();
    Ok(found)
}

/// The data directories as one document: each file's document at the path
/// of its directory, as conftest's `--data` puts it.
fn data(root: &Path, sources: &[PathBuf]) -> Result<Value, Error> {
    let mut data = Value::Object(Map::new());
    for source in sources {
        let files = files(root, source, &["json", "yaml", "yml"], "data")?;
        let source = root.join(source);
        for path in files {
            let language = if path.extension().is_some_and(|e| e == "json") {
                "json"
            } else {
                "yaml"
            };
            let text = std::fs::read_to_string(&path).map_err(|e| Error::Io(path.clone(), e))?;
            let documents = document::documents(language, &path, &text)?.unwrap_or_default();
            let mut at = &mut data;
            let dir = path.parent().and_then(|dir| dir.strip_prefix(&source).ok());
            for part in dir.into_iter().flat_map(Path::components) {
                let key = part.as_os_str().to_string_lossy().into_owned();
                at = object(at, &path)?
                    .entry(key)
                    .or_insert_with(|| Value::Object(Map::new()));
            }
            for document in documents {
                let Value::Object(entries) = document.value else {
                    return Err(Error::Other(format!(
                        "{}: data is an object at its root",
                        path.display()
                    )));
                };
                object(at, &path)?.extend(entries);
            }
        }
    }
    Ok(data)
}

fn object<'a>(value: &'a mut Value, path: &Path) -> Result<&'a mut Map<String, Value>, Error> {
    value.as_object_mut().ok_or_else(|| {
        Error::Other(format!(
            "{}: its directory is a key another data file holds a value at",
            path.display()
        ))
    })
}

/// The packages that run, by language: a package named after a language,
/// or bound to one by the config. `main` unbound is an error; any other
/// package is a library.
fn packages(
    engine: &mut Engine,
    bound: &std::collections::BTreeMap<String, String>,
) -> Result<HashMap<&'static str, Vec<Package>>, Error> {
    for (package, language) in bound {
        if !LANGUAGES.contains(&language.as_str()) {
            return Err(Error::Other(format!(
                "the config binds package `{package}` to `{language}`, which is no language \
                 zoll knows; `zoll languages` lists them"
            )));
        }
    }
    // A package may span several files: its rules are gathered first.
    let mut rules: HashMap<String, (BTreeSet<String>, String)> = HashMap::new();
    for module in engine.get_modules() {
        let name = module.package.refr.span().text().to_owned();
        let file = module.package.span.source.file().clone();
        let entry = rules.entry(name).or_insert_with(|| (BTreeSet::new(), file));
        for rule in &module.policy {
            let head = match &**rule {
                Rule::Spec {
                    head: RuleHead::Compr { refr, .. } | RuleHead::Set { refr, .. },
                    ..
                }
                | Rule::Default { refr, .. } => refr.span().text(),
                Rule::Spec {
                    head: RuleHead::Func { .. },
                    ..
                } => continue,
            };
            let name = head.split(['.', '[']).next().unwrap_or(head);
            entry.0.insert(name.to_owned());
        }
    }
    let mut runs: HashMap<&'static str, Vec<Package>> = HashMap::new();
    for (name, (names, file)) in rules {
        let language = bound
            .get(&name)
            .map(String::as_str)
            .or_else(|| LANGUAGES.iter().find(|l| **l == name).copied());
        let Some(language) = language.and_then(|l| LANGUAGES.iter().find(|k| **k == l)) else {
            if name == "main" {
                return Err(Error::Other(format!(
                    "{file}: package main does not name a language; use the language it \
                     checks, e.g. `package yaml`, or bind it in the config: \
                     `[packages] main = \"yaml\"`, under `[tool.zoll.packages]` in \
                     `pyproject.toml`"
                )));
            }
            continue;
        };
        runs.entry(language).or_default().push(Package {
            rules: names
                .iter()
                .filter_map(|rule| severity(rule).map(|s| (rule.clone(), s)))
                .collect(),
            has_exception: names.contains("exception"),
            name,
        });
    }
    for packages in runs.values_mut() {
        packages.sort_by(|a, b| a.name.cmp(&b.name));
    }
    Ok(runs)
}

/// A rule's weight by its name, as conftest reads it:
/// `^(deny|violation)(_[a-zA-Z0-9]+)*$` and `^warn(_[a-zA-Z0-9]+)*$`.
fn severity(rule: &str) -> Option<Severity> {
    let mut parts = rule.split('_');
    let severity = match parts.next()? {
        "deny" | "violation" => Severity::Deny,
        "warn" => Severity::Warn,
        _ => return None,
    };
    parts
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric()))
        .then_some(severity)
}

/// `deny_curl_pipe` is `curl_pipe`; a bare `deny` is empty.
fn suffix(rule: &str) -> &str {
    rule.split_once('_').map_or("", |(_, suffix)| suffix)
}

/// The rule names `exception` holds: a set of sets of names, as conftest
/// queries `exception[_][_]`.
fn exceptions(engine: &mut Engine, package: &str) -> Result<BTreeSet<String>, Error> {
    let value = evaluate_rule(engine, &format!("data.{package}.exception"))?;
    Ok(value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|name| name.as_str().map(str::to_owned))
        .collect())
}

/// The items a rule yields: it must be a set, as conftest requires.
fn results(engine: &mut Engine, package: &str, rule: &str) -> Result<Vec<Value>, Error> {
    match evaluate_rule(engine, &format!("data.{package}.{rule}"))? {
        Value::Array(items) => Ok(items),
        Value::Null => Ok(Vec::new()),
        other => Err(Error::Other(format!(
            "`{package}.{rule}` is {other}, not a set; write `{rule} contains msg if {{ ... }}`"
        ))),
    }
}

/// A rule's value as JSON, a set as an array; null when it is undefined.
fn evaluate_rule(engine: &mut Engine, rule: &str) -> Result<Value, Error> {
    let value = engine
        .eval_rule(rule.to_owned())
        .map_err(|e| Error::Other(format!("{rule}: {e}")))?;
    if value == regorus::Value::Undefined {
        return Ok(Value::Null);
    }
    serde_json::to_value(&value).map_err(|e| Error::Other(format!("{rule}: {e}")))
}

fn violation(
    result: Value,
    rule: String,
    severity: Severity,
    package: &str,
    name: &str,
) -> Result<Violation, Error> {
    let mut metadata = match result {
        Value::String(message) => {
            return Ok(Violation {
                rule,
                severity,
                message,
                at: None,
                line: None,
                metadata: Map::new(),
            });
        }
        Value::Object(metadata) => metadata,
        other => {
            return Err(Error::Other(format!(
                "`{package}.{name}` yields {other}; a result is a string or an object with `msg`"
            )));
        }
    };
    let Some(Value::String(message)) = metadata.remove("msg") else {
        return Err(Error::Other(format!(
            "`{package}.{name}` yields an object without a string `msg`, which conftest \
             requires"
        )));
    };
    let at = match metadata.remove("at") {
        Some(value) => {
            let at = At::from_json(&value);
            if at.is_none() {
                // Not a path, not a node: the result's own key, kept.
                metadata.insert("at".to_owned(), value);
            }
            at
        }
        None => None,
    };
    let line = metadata
        .remove("_loc")
        .and_then(|loc| loc.get("line").and_then(Value::as_u64))
        .and_then(|line| usize::try_from(line).ok());
    Ok(Violation {
        rule,
        severity,
        message,
        at,
        line,
        metadata,
    })
}
