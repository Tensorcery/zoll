//! The reports (docs/design.md, section 12): `text`, each violation as rustc
//! draws a diagnostic; `json`, one object per violation; `github`, workflow
//! commands that annotate a PR's diff; `sarif`, SARIF 2.1.0 for code
//! scanning.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::Path;

use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};
use serde_json::{Value, json};
use zoll_core::check::{Checked, Found};
use zoll_core::config::FailOn;
use zoll_core::document::Place;
use zoll_core::policy::{Outcome, Severity, Test};

use crate::{Format, TestFormat, shown};

/// Writes the report; `true` when what was found fails the run.
pub(crate) fn write(
    format: Format,
    checked: &[Checked],
    cwd: &Path,
    fail_on: FailOn,
    out: &mut impl Write,
) -> io::Result<bool> {
    let results = results(checked, cwd);
    let count = |severity| {
        results
            .iter()
            .filter(|r| r.found.violation.severity == severity)
            .count()
    };
    let fails = count(Severity::Deny) > 0 || (fail_on == FailOn::Warn && count(Severity::Warn) > 0);
    let written = match format {
        Format::Text => text(&results, out),
        Format::Json => {
            let all: Vec<Value> = results.iter().map(Shown::json).collect();
            serde_json::to_writer_pretty(&mut *out, &all)
                .map_err(io::Error::from)
                .and_then(|()| writeln!(out))
        }
        Format::Github => github(&results, out),
        Format::Sarif => serde_json::to_writer_pretty(&mut *out, &sarif(&results))
            .map_err(io::Error::from)
            .and_then(|()| writeln!(out)),
    };
    match written {
        // `zoll check | head`: the reader stopped reading, and the run still
        // found what it found.
        Err(error) if error.kind() != io::ErrorKind::BrokenPipe => Err(error),
        _ => Ok(fails),
    }
}

/// A result, with what every report needs of its file.
struct Shown<'a> {
    found: &'a Found,
    /// The path as the user would type it.
    path: String,
    /// Its file's text.
    text: &'a str,
    /// Where it ends.
    end: Place,
}

impl Shown<'_> {
    fn json(&self) -> Value {
        let violation = &self.found.violation;
        let place = &self.found.place;
        let mut object = json!({
            "rule": violation.rule,
            "severity": severity(violation.severity),
            "message": violation.message,
            "path": self.path,
            "start": {"line": place.line, "column": place.column},
            "end": {"line": self.end.line, "column": self.end.column},
            "fragments": self.found.fragments,
        });
        if !violation.metadata.is_empty()
            && let Some(object) = object.as_object_mut()
        {
            object.insert(
                "metadata".to_owned(),
                Value::Object(violation.metadata.clone()),
            );
        }
        object
    }
}

fn results<'a>(checked: &'a [Checked], cwd: &Path) -> Vec<Shown<'a>> {
    checked
        .iter()
        .flat_map(|file| {
            let path = shown(&file.file.path, cwd);
            file.found.iter().map(move |found| Shown {
                found,
                path: path.clone(),
                text: &file.text,
                end: Place::of(&file.text, found.place.span.end..found.place.span.end),
            })
        })
        .collect()
}

fn severity(severity: Severity) -> &'static str {
    match severity {
        Severity::Deny => "deny",
        Severity::Warn => "warn",
        Severity::Notice => "notice",
    }
}

fn text(results: &[Shown<'_>], out: &mut impl Write) -> io::Result<()> {
    let renderer = Renderer::styled();
    for result in results {
        let violation = &result.found.violation;
        let level = match violation.severity {
            Severity::Deny => Level::ERROR,
            Severity::Warn => Level::WARNING,
            Severity::Notice => Level::NOTE,
        };
        let snippet = Snippet::source(result.text)
            .path(result.path.as_str())
            .annotation(AnnotationKind::Primary.span(result.found.place.span.clone()));
        let title = level
            .primary_title(violation.message.as_str())
            .id(violation.rule.as_str());
        writeln!(out, "{}\n", renderer.render(&[title.element(snippet)]))?;
    }
    let count = |severity| {
        results
            .iter()
            .filter(|r| r.found.violation.severity == severity)
            .count()
    };
    let summary = [
        (count(Severity::Deny), "violation", "violations"),
        (count(Severity::Warn), "warning", "warnings"),
        (count(Severity::Notice), "notice", "notices"),
    ]
    .into_iter()
    .filter(|(n, _, _)| *n > 0)
    .map(|(n, one, many)| format!("{n} {}", if n == 1 { one } else { many }))
    .collect::<Vec<_>>()
    .join(", ");
    if !summary.is_empty() {
        writeln!(out, "{summary}")?;
    }
    Ok(())
}

/// `::error file=a.sh,line=1,col=1,endLine=1,endColumn=47,title=bash.curl_pipe::curl piped into a shell`
fn github(results: &[Shown<'_>], out: &mut impl Write) -> io::Result<()> {
    // GitHub's escapes: the message's, then a property's.
    let data = |s: &str| {
        s.replace('%', "%25")
            .replace('\r', "%0D")
            .replace('\n', "%0A")
    };
    let property = |s: &str| data(s).replace(':', "%3A").replace(',', "%2C");
    for result in results {
        let violation = &result.found.violation;
        let place = &result.found.place;
        let command = match violation.severity {
            Severity::Deny => "error",
            Severity::Warn => "warning",
            Severity::Notice => "notice",
        };
        writeln!(
            out,
            "::{command} file={},line={},col={},endLine={},endColumn={},title={}::{}",
            property(&result.path),
            place.line,
            place.column,
            result.end.line,
            result.end.column,
            property(&violation.rule),
            data(&violation.message),
        )?;
    }
    Ok(())
}

/// One run of zoll in SARIF 2.1.0: its rules, and each result at its region.
fn sarif(results: &[Shown<'_>]) -> Value {
    let mut rules: BTreeMap<&str, usize> = BTreeMap::new();
    for result in results {
        rules.insert(&result.found.violation.rule, 0);
    }
    for (index, value) in rules.values_mut().enumerate() {
        *value = index;
    }
    let sarif_results: Vec<Value> = results
        .iter()
        .map(|result| {
            let violation = &result.found.violation;
            let place = &result.found.place;
            json!({
                "ruleId": violation.rule,
                "ruleIndex": rules.get(violation.rule.as_str()),
                "level": match violation.severity {
                    Severity::Deny => "error",
                    Severity::Warn => "warning",
                    Severity::Notice => "note",
                },
                "message": {"text": violation.message},
                "locations": [{"physicalLocation": {
                    "artifactLocation": {"uri": uri(&result.path)},
                    "region": {
                        "startLine": place.line,
                        "startColumn": place.column,
                        "endLine": result.end.line,
                        "endColumn": result.end.column,
                    },
                }}],
            })
        })
        .collect();
    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {"driver": {
                "name": "zoll",
                "version": env!("CARGO_PKG_VERSION"),
                "informationUri": "https://github.com/Tensorcery/zoll",
                "rules": rules.keys().map(|id| json!({"id": id})).collect::<Vec<_>>(),
            }},
            // As zoll counts columns everywhere.
            "columnKind": "unicodeCodePoints",
            "results": sarif_results,
        }],
    })
}

/// A path as a URI reference: `/` between its parts, any other byte outside
/// RFC 3986's unreserved ones percent-encoded. A path outside the working
/// directory is absolute, and a `file:` URI.
fn uri(path: &str) -> String {
    let path = path.replace('\\', "/");
    let absolute = Path::new(&path).is_absolute() || path.starts_with('/');
    let mut uri = String::with_capacity(path.len());
    if absolute {
        // `file:///C:/x` on Windows, `file:///x` elsewhere.
        uri.push_str(if path.starts_with('/') {
            "file://"
        } else {
            "file:///"
        });
    }
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) || (absolute && byte == b':') {
            uri.push(char::from(byte));
        } else {
            // Writing to a String cannot fail.
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri
}

/// The tests' report: in text, each test that did not pass, at its file and
/// line, then a count; in JSON, every test.
pub(crate) fn tests(
    format: TestFormat,
    tests: &[Test],
    root: &Path,
    cwd: &Path,
    out: &mut impl Write,
) -> io::Result<()> {
    let path = |test: &Test| shown(&root.join(&test.file), cwd);
    if let TestFormat::Json = format {
        let all: Vec<Value> = tests
            .iter()
            .map(|test| {
                let (result, error) = match &test.outcome {
                    Outcome::Pass => ("pass", None),
                    Outcome::Fail => ("fail", None),
                    Outcome::Error(e) => ("error", Some(e.trim())),
                };
                json!({
                    "package": test.package,
                    "rule": test.name,
                    "path": path(test),
                    "line": test.line,
                    "result": result,
                    "error": error,
                })
            })
            .collect();
        serde_json::to_writer_pretty(&mut *out, &all)?;
        return writeln!(out);
    }
    for test in tests {
        let id = format!("{}.{}", test.package, test.name);
        match &test.outcome {
            Outcome::Pass => {}
            Outcome::Fail => writeln!(out, "FAIL {id}  {}:{}", path(test), test.line)?,
            Outcome::Error(e) => writeln!(
                out,
                "ERROR {id}  {}:{}\n{}",
                path(test),
                test.line,
                e.trim()
            )?,
        }
    }
    let failed = tests.iter().filter(|t| t.outcome != Outcome::Pass).count();
    match (tests.len(), failed) {
        (0, _) => writeln!(out, "no tests"),
        (1, 0) => writeln!(out, "1 test passed"),
        (n, 0) => writeln!(out, "{n} tests passed"),
        (1, _) => writeln!(out, "1 test, 1 failed"),
        (n, failed) => writeln!(out, "{n} tests, {failed} failed"),
    }
}
