//! The `text` report: each violation as rustc draws a diagnostic - the
//! severity, the rule id and the message, then `path:line:col` and the line,
//! marked.

use std::io::{self, Write};
use std::path::Path;

use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};
use zoll_core::check::Checked;
use zoll_core::config::FailOn;
use zoll_core::policy::Severity;

use crate::shown;

/// Writes the report; `true` when what was found fails the run.
pub(crate) fn text(
    checked: &[Checked],
    cwd: &Path,
    fail_on: FailOn,
    out: &mut impl Write,
) -> io::Result<bool> {
    let renderer = Renderer::styled();
    let (mut denied, mut warned, mut noted) = (0, 0, 0);
    for file in checked {
        let path = shown(&file.file.path, cwd);
        for found in &file.found {
            let level = match found.violation.severity {
                Severity::Deny => {
                    denied += 1;
                    Level::ERROR
                }
                Severity::Warn => {
                    warned += 1;
                    Level::WARNING
                }
                Severity::Notice => {
                    noted += 1;
                    Level::NOTE
                }
            };
            let snippet = Snippet::source(file.text.as_str())
                .path(path.as_str())
                .annotation(AnnotationKind::Primary.span(found.place.span.clone()));
            let title = level
                .primary_title(found.violation.message.as_str())
                .id(found.violation.rule.as_str());
            writeln!(out, "{}\n", renderer.render(&[title.element(snippet)]))?;
        }
    }
    if denied + warned + noted > 0 {
        writeln!(out, "{}", summary(denied, warned, noted))?;
    }
    Ok(denied > 0 || (fail_on == FailOn::Warn && warned > 0))
}

/// `2 violations, 1 warning`: each kind there is, counted.
fn summary(denied: usize, warned: usize, noted: usize) -> String {
    [
        (denied, "violation", "violations"),
        (warned, "warning", "warnings"),
        (noted, "notice", "notices"),
    ]
    .into_iter()
    .filter(|(n, _, _)| *n > 0)
    .map(|(n, one, many)| format!("{n} {}", if n == 1 { one } else { many }))
    .collect::<Vec<_>>()
    .join(", ")
}
