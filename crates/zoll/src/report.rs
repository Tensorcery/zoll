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
    let (mut denied, mut warned) = (0, 0);
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
    if denied + warned > 0 {
        writeln!(out, "{}", summary(denied, warned))?;
    }
    Ok(denied > 0 || (fail_on == FailOn::Warn && warned > 0))
}

fn summary(denied: usize, warned: usize) -> String {
    let count =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    match (denied, warned) {
        (_, 0) => count(denied, "violation", "violations"),
        (0, _) => count(warned, "warning", "warnings"),
        _ => format!(
            "{}, {}",
            count(denied, "violation", "violations"),
            count(warned, "warning", "warnings")
        ),
    }
}
