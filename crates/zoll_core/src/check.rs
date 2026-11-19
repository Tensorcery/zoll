//! A run: every file through its documents and its language's policies, in
//! parallel, each violation placed in its file.

use rayon::prelude::*;

use crate::document::{self, Place};
use crate::error::Error;
use crate::input;
use crate::policy::{Policies, Severity, Violation};
use crate::walk::File;

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
}

/// The rule id of a data file that does not parse (docs/design.md,
/// section 14).
pub const PARSE: &str = "zoll.parse";

/// Every file checked; those with nothing found are left out.
///
/// # Errors
///
/// A file that cannot be read, a rule that fails to evaluate.
pub fn check(files: Vec<File>, policies: &Policies) -> Result<Vec<Checked>, Error> {
    let checked: Result<Vec<_>, Error> = files
        .into_par_iter()
        .map(|file| check_file(file, policies))
        .collect();
    let mut checked: Vec<Checked> = checked?.into_iter().flatten().collect();
    checked.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(checked)
}

fn check_file(file: File, policies: &Policies) -> Result<Option<Checked>, Error> {
    if !matches!(file.language, "yaml" | "json" | "toml") {
        // Code is parsed from stage 4 of docs/plan.md on.
        return Ok(None);
    }
    let text = input::read(&file)?;
    let mut found = Vec::new();
    match input::evaluations(&file, &text) {
        Ok(evaluations) => {
            for evaluation in &evaluations {
                for violation in policies.check(file.language, evaluation)? {
                    let place = match (&violation.at, violation.line) {
                        (Some(at), _) => {
                            document::locate(file.language, &text, evaluation.origin, at)
                        }
                        (None, Some(line)) => line_start(&text, line),
                        (None, None) => {
                            document::locate(file.language, &text, evaluation.origin, &[])
                        }
                    };
                    found.push(Found { violation, place });
                }
            }
        }
        // A data file that does not parse - a Helm template, `${{ }}` that
        // breaks the YAML - is a warning, not the end of the run.
        Err(Error::Source(error)) => found.push(Found {
            place: Place::of(&text, error.span.unwrap_or(0..0)),
            violation: Violation {
                rule: PARSE.to_owned(),
                severity: Severity::Warn,
                message: format!("does not parse as {}: {}", file.language, error.message),
                at: None,
                line: None,
                metadata: serde_json::Map::new(),
            },
        }),
        Err(error) => return Err(error),
    }
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

/// The start of a line, from 1; the end of the text past its last line.
fn line_start(text: &str, line: usize) -> Place {
    let start: usize = text
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::len)
        .sum();
    Place::of(text, start..start)
}
