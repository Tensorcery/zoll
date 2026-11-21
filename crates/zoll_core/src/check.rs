//! A run: every file through its documents and its language's policies, in
//! parallel, each violation placed in its file.

use std::ops::Range;

use rayon::prelude::*;
use serde_json::{Value, json};

use crate::document::{self, Place};
use crate::error::{Error, line_column_span};
use crate::input::{self, Evaluation};
use crate::language;
use crate::policy::{At, Policies, Severity, Violation};
use crate::suppress::{self, PerFile};
use crate::walk::File;

pub use crate::input::{PARSE, UNCHECKED};
pub use crate::policy::{Checked, Found};

/// Every file checked, less what `per_file` and the files' comments
/// silence; those with nothing found are left out.
///
/// # Errors
///
/// A file that cannot be read, a rule that fails to evaluate.
pub fn check(
    files: Vec<File>,
    policies: &Policies,
    per_file: &PerFile,
) -> Result<Vec<Checked>, Error> {
    let checked: Result<Vec<_>, Error> = files
        .into_par_iter()
        .map(|file| check_file(file, policies, per_file))
        .collect();
    let mut checked: Vec<Checked> = checked?.into_iter().flatten().collect();
    checked.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(checked)
}

/// A bigger file is not read: its trees would take gigabytes.
const MAX_FILE_SIZE: u64 = 16 << 20;

fn check_file(
    file: File,
    policies: &Policies,
    per_file: &PerFile,
) -> Result<Option<Checked>, Error> {
    if !language::compiled(file.language) {
        // A language this zoll was built without: nothing to check it with.
        return Ok(None);
    }
    let size = std::fs::metadata(&file.path).map_or(0, |meta| meta.len());
    if size > MAX_FILE_SIZE {
        let why = format!(
            "this file is not checked: it is more than {} MiB",
            MAX_FILE_SIZE >> 20
        );
        let found = vec![note(UNCHECKED, Severity::Notice, Place::of("", 0..0), why)];
        return Ok(Some(Checked {
            file,
            text: String::new(),
            found,
        }));
    }
    let text = match input::read(&file) {
        Ok(text) => text,
        // Latin-1, UTF-16: not text zoll can parse, so a warning, as a file
        // that does not parse is.
        Err(Error::Io(_, error)) if error.kind() == std::io::ErrorKind::InvalidData => {
            let why = "is not UTF-8 text".to_owned();
            let found = vec![note(PARSE, Severity::Warn, Place::of("", 0..0), why)];
            return Ok(Some(Checked {
                file,
                text: String::new(),
                found,
            }));
        }
        Err(error) => return Err(error),
    };
    let mut found = Vec::new();
    let evaluations = input::evaluations(&file, &text);
    let evaluated = evaluations.is_ok();
    match evaluations {
        Ok((evaluations, notes)) => {
            for evaluation in &evaluations {
                let source = &evaluation.source;
                // A rule that fails on one file: the file is named, for the
                // user to see what it failed on.
                let violations = policies
                    .check(source.language, evaluation)
                    .map_err(|error| match error {
                        Error::Other(message) => {
                            Error::Other(format!("checking {}: {message}", file.relative))
                        }
                        error => error,
                    })?;
                for violation in violations {
                    let span = local_span(&violation, evaluation);
                    let place = Place::of(&text, source.map.span(span));
                    let fragments = chain(&evaluation.data["zoll"]["file"]);
                    found.push(Found {
                        violation,
                        place,
                        fragments,
                    });
                }
            }
            for note in notes {
                found.push(Found {
                    place: Place::of(&text, note.span),
                    fragments: Vec::new(),
                    violation: Violation {
                        rule: note.rule.to_owned(),
                        severity: note.severity,
                        message: note.message,
                        at: None,
                        line: None,
                        metadata: serde_json::Map::new(),
                    },
                });
            }
        }
        // A data file that does not parse - a Helm template, `${{ }}` that
        // breaks the YAML - is a warning, not the end of the run.
        Err(Error::Source(error)) => found.push(note(
            PARSE,
            Severity::Warn,
            Place::of(&text, error.span.unwrap_or(0..0)),
            format!("does not parse as {}: {}", file.language, error.message),
        )),
        Err(error) => return Err(error),
    }
    suppress::apply(&file, &text, &mut found, per_file, evaluated);
    if found.is_empty() {
        return Ok(None);
    }
    found.sort_by(|a, b| {
        (a.place.line, a.place.column, &a.violation.rule).cmp(&(
            b.place.line,
            b.place.column,
            &b.violation.rule,
        ))
    });
    Ok(Some(Checked { file, text, found }))
}

/// What zoll itself says of a file it could not read as its language.
fn note(rule: &str, severity: Severity, place: Place, message: String) -> Found {
    Found {
        place,
        fragments: Vec::new(),
        violation: Violation {
            rule: rule.to_owned(),
            severity,
            message,
            at: None,
            line: None,
            metadata: serde_json::Map::new(),
        },
    }
}

/// The fragments `data.zoll.file` was reached through, the outermost
/// first.
fn chain(file: &Value) -> Vec<Value> {
    let host = &file["host"];
    if host.is_null() {
        return Vec::new();
    }
    let mut chain = chain(host);
    chain.push(json!({"language": file["language"], "at": host["at"]}));
    chain
}

/// The bytes a violation points at, in the text of its evaluation: its
/// path, its node, its line, or the start of the document.
fn local_span(violation: &Violation, evaluation: &Evaluation) -> Range<usize> {
    let source = &evaluation.source;
    let text = source.text.as_str();
    match (&violation.at, violation.line) {
        (Some(At::Path(path)), _) => index(source).locate(text, evaluation.origin, path).span,
        (Some(At::Node { start, end }), _) => {
            let byte = |(line, column)| line_column_span(text, line, column).start;
            byte(*start)..byte(*end)
        }
        (None, Some(line)) => line_start(text, line),
        (None, None) => index(source).locate(text, evaluation.origin, &[]).span,
    }
}

fn index(source: &input::Source) -> &document::Index {
    source
        .index
        .get_or_init(|| document::Index::new(source.language, &source.text))
}

/// The start of a line, from 1; the end of the text past its last line.
fn line_start(text: &str, line: usize) -> Range<usize> {
    let start: usize = text
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::len)
        .sum();
    start..start
}
