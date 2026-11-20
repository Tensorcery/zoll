//! Results the user silenced: by `per-file-ignores` in the config, or by a
//! `zoll: ignore[rule-id]` comment on the line a result is reported at; and
//! the suppressions that silenced nothing (docs/design.md, section 9).

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::Path;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

use globset::GlobMatcher;
use regex::Regex;
use toml::Spanned;

use crate::check::{Checked, Found};
use crate::document::Place;
use crate::error::Error;
use crate::language::glob_of;
use crate::policy::{Severity, Violation};
use crate::walk::{self, File};

/// The rule id of a suppression that silenced nothing.
pub const UNUSED: &str = "zoll.unused_ignore";

/// `zoll: ignore[a, b]` after a comment opener of any common syntax: `#`,
/// `//`, `--`, `;`, `%`, `/*`, `<!--`. One list serves every language, and the
/// host's and the fragment's alike: bash in a YAML `run:` takes `#` either way.
static MARKER: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"(?:#|//|--|;|%|/\*|<!--)[ \t]*zoll:[ \t]*ignore\[([^\]\n]*)\]").ok()
});

/// A rule id in a `zoll: ignore[...]` comment.
#[derive(Debug)]
pub struct Inline {
    /// The line, from 1.
    line: usize,
    /// The rule id.
    rule: String,
    /// Its bytes in the file.
    span: Range<usize>,
    /// Whether it silenced a result.
    used: bool,
}

/// The rule ids the comments of a file name, line by line.
#[must_use]
pub fn inline(text: &str) -> Vec<Inline> {
    let Some(marker) = MARKER.as_ref().filter(|_| text.contains("zoll")) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut start = 0;
    for (i, line) in text.split_inclusive('\n').enumerate() {
        for list in marker.captures_iter(line).filter_map(|c| c.get(1)) {
            let mut at = start + list.start();
            for rule in list.as_str().split(',') {
                let trimmed = rule.trim();
                let offset = at + (rule.len() - rule.trim_start().len());
                if !trimmed.is_empty() {
                    found.push(Inline {
                        line: i + 1,
                        rule: trimmed.to_owned(),
                        span: offset..offset + trimmed.len(),
                        used: false,
                    });
                }
                at += rule.len() + 1;
            }
        }
        start += line.len();
    }
    found
}

/// Drops what the comments of `text` or `per-file-ignores` silence, then
/// reports each comment's rule id that silenced nothing. Those reports are
/// not silenced themselves. `checked` is false when the file was not
/// evaluated (it does not parse): its comments are not judged then.
pub(crate) fn apply(
    file: &File,
    text: &str,
    found: &mut Vec<Found>,
    per_file: &PerFile,
    checked: bool,
) {
    let mut inline = inline(text);
    found.retain(|f| {
        let rule = f.violation.rule.as_str();
        let mut by_comment = false;
        for i in inline
            .iter_mut()
            .filter(|i| i.line == f.place.line && i.rule == rule)
        {
            i.used = true;
            by_comment = true;
        }
        // Both are marked used when both silence it.
        let by_config = per_file.silences(&file.relative, rule);
        !(by_comment || by_config)
    });
    if !checked {
        return;
    }
    for i in inline.into_iter().filter(|i| !i.used) {
        found.push(unused(
            text,
            i.span,
            format!("`zoll: ignore[{}]` silences nothing on this line", i.rule),
        ));
    }
}

fn unused(text: &str, span: Range<usize>, message: String) -> Found {
    Found {
        place: Place::of(text, span),
        violation: Violation {
            rule: UNUSED.to_owned(),
            severity: Severity::Warn,
            message,
            at: None,
            line: None,
            metadata: serde_json::Map::new(),
        },
    }
}

/// The config's `per-file-ignores`, each rule id marked once it silences a
/// result.
#[derive(Debug, Default)]
pub struct PerFile {
    entries: Vec<Entry>,
}

#[derive(Debug)]
struct Entry {
    glob: String,
    matcher: GlobMatcher,
    rules: Vec<(Spanned<String>, AtomicBool)>,
}

impl PerFile {
    /// The table, its globs compiled.
    ///
    /// # Errors
    ///
    /// A glob that does not compile.
    pub fn new(table: &BTreeMap<String, Vec<Spanned<String>>>) -> Result<Self, Error> {
        let entries = table
            .iter()
            .map(|(glob, rules)| {
                Ok(Entry {
                    glob: glob.clone(),
                    matcher: glob_of(glob)?.compile_matcher(),
                    rules: rules
                        .iter()
                        .map(|rule| (rule.clone(), AtomicBool::new(false)))
                        .collect(),
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self { entries })
    }

    /// Whether `rule` is not reported in the file at `relative`; each entry
    /// that silences it is marked used.
    fn silences(&self, relative: &str, rule: &str) -> bool {
        let mut silenced = false;
        for entry in self.entries.iter().filter(|e| e.matcher.is_match(relative)) {
            for (_, used) in entry.rules.iter().filter(|(r, _)| r.get_ref() == rule) {
                used.store(true, Ordering::Relaxed);
                silenced = true;
            }
        }
        silenced
    }

    /// Reports each rule id of the table that silenced nothing, in the config
    /// file at `path`: a warning on its line, the file added to `checked`.
    /// Only a run over the whole project can tell: one over some files (a
    /// pre-commit hook's) has not seen the rest.
    ///
    /// # Errors
    ///
    /// The config file cannot be read.
    pub fn report_unused(
        &self,
        path: &Path,
        root: &Path,
        checked: &mut Vec<Checked>,
    ) -> Result<(), Error> {
        let mut unused_rules = self.entries.iter().flat_map(|entry| {
            entry
                .rules
                .iter()
                .filter(|(_, used)| !used.load(Ordering::Relaxed))
                .map(move |(rule, _)| (entry, rule))
        });
        let Some(first) = unused_rules.next() else {
            return Ok(());
        };
        let text = std::fs::read_to_string(path).map_err(|e| Error::Io(path.to_owned(), e))?;
        let found = std::iter::once(first)
            .chain(unused_rules)
            .map(|(entry, rule)| {
                let message = format!("`{}` silences {} in no file", entry.glob, rule.get_ref());
                unused(&text, rule.span(), message)
            });
        if !checked.iter().any(|c| c.file.path == path) {
            checked.push(Checked {
                file: File {
                    path: path.to_owned(),
                    relative: walk::relative(path, root),
                    language: "toml",
                },
                text: text.clone(),
                found: Vec::new(),
            });
            checked.sort_by(|a, b| a.file.cmp(&b.file));
        }
        if let Some(file) = checked.iter_mut().find(|c| c.file.path == path) {
            file.found.extend(found);
            file.found.sort_by_key(|f| f.place.span.start);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_of_any_syntax_name_rules() {
        let text = "a\ncurl x | sh  # zoll: ignore[bash.curl_pipe, b]\n// zoll:ignore[c]\nx = \"zoll: ignore[d]\"\n";
        let found: Vec<_> = inline(text)
            .into_iter()
            .map(|i| (i.line, i.rule.clone(), text[i.span].to_owned()))
            .collect();
        assert_eq!(
            found,
            [
                (2, "bash.curl_pipe".to_owned(), "bash.curl_pipe".to_owned()),
                (2, "b".to_owned(), "b".to_owned()),
                (3, "c".to_owned(), "c".to_owned()),
            ]
        );
    }
}
