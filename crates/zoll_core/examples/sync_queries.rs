//! Copies the injection queries of Helix into `queries/` at the commit
//! below, less what zoll does not evaluate, and lists what it left out
//! (docs/design.md, sections 6 and 14; queries/README.md). `make queries`
//! runs it; the diff it leaves is reviewed like any other.
//!
//! An injection is left out when it names a language zoll does not have,
//! uses a predicate zoll does not evaluate, or does not compile against the
//! grammar zoll is built with. Editor-only directives are stripped.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use tree_sitter::Query;
use zoll_core::code;
use zoll_core::injection::{SUPPORTED, from_helix, helix_name};
use zoll_core::language::LANGUAGES;

/// The Helix commit the queries come from.
const HELIX: &str = "9de242bfac2aac9dfd0e4496eb25190e85c24e69";

/// Directives that only move an editor's highlight, stripped from a pattern.
const EDITOR_ONLY: [&str; 1] = ["offset!"];

/// Languages whose injections zoll writes in full in `zoll.scm`: Helix's
/// miss what a policy must see (YAML's quoted and flow-style `run:`, a
/// `command:` list's `-c` script), and a Helix rule would add wrong ones.
const OWNED: [&str; 1] = ["yaml"];

type Result<T> = std::result::Result<T, String>;

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(io::stderr(), "sync_queries: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let helix = checkout(&root.join("target/queries-src"))?;
    let queries = root.join("queries");
    let mut out = io::stdout().lock();
    for language in LANGUAGES {
        let Some((grammar, _)) = code::grammar(language) else {
            continue;
        };
        let dir = queries.join(language);
        let file = dir.join("injections.scm");
        let name = helix_name(language);
        let source = helix
            .join("runtime/queries")
            .join(name)
            .join("injections.scm");
        if !source.exists() || OWNED.contains(&language) {
            remove(&file)?;
            continue;
        }
        let text = inherited(&helix.join("runtime/queries"), name, 0)?;
        let mut kept = String::new();
        let mut patterns = 0;
        for item in items(&text) {
            let Item::Pattern(pattern) = item else {
                kept.push_str(item.text());
                continue;
            };
            let pattern = strip_editor_only(pattern);
            if let Some(why) = left_out(&pattern, &grammar) {
                let first = pattern.lines().next().unwrap_or_default();
                writeln!(out, "{language}: left out {first} ... ({why})")
                    .map_err(|e| e.to_string())?;
            } else {
                kept.push_str(&pattern);
                patterns += 1;
            }
        }
        if patterns == 0 {
            remove(&file)?;
            continue;
        }
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let header = format!(
            "; Copied by `make queries` from Helix's runtime/queries/{name}/injections.scm\n\
             ; at {HELIX}, less what zoll does not evaluate\n\
             ; (queries/README.md).\n;\n{MPL}\n\n"
        );
        write(&file, &(header + &tidy(&kept)))?;
    }
    write(&queries.join("languages.toml"), &languages(&helix)?)
}

/// The MPL's notice, which a copied file carries.
const MPL: &str = "; This Source Code Form is subject to the terms of the Mozilla Public\n\
                   ; License, v. 2.0. If a copy of the MPL was not distributed with this\n\
                   ; file, You can obtain one at https://mozilla.org/MPL/2.0/.";

/// Helix at [`HELIX`], its queries and `languages.toml` only, fetched once
/// into `cache`.
fn checkout(cache: &Path) -> Result<PathBuf> {
    let dir = cache.join(format!("helix-{HELIX}"));
    if dir.join("languages.toml").exists() {
        return Ok(dir);
    }
    // What a run stopped mid-fetch left: started over, or `git remote add`
    // fails on it every time after.
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let git = |args: &[&str]| -> Result<()> {
        let status = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .status()
            .map_err(|e| format!("git: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("git {} failed", args.join(" ")))
        }
    };
    git(&["init", "-q"])?;
    git(&[
        "remote",
        "add",
        "origin",
        "https://github.com/helix-editor/helix.git",
    ])?;
    git(&[
        "sparse-checkout",
        "set",
        "--no-cone",
        "/runtime/queries/",
        "/languages.toml",
    ])?;
    git(&[
        "fetch",
        "-q",
        "--depth",
        "1",
        "--filter=blob:none",
        "origin",
        HELIX,
    ])?;
    git(&["checkout", "-q", "FETCH_HEAD"])?;
    Ok(dir)
}

/// The file of `name` with each `; inherits: a,b` line replaced by those
/// languages' files, as Helix reads it.
fn inherited(queries: &Path, name: &str, depth: usize) -> Result<String> {
    if depth > 8 {
        return Err(format!("{name}: inherits too deep"));
    }
    let path = queries.join(name).join("injections.scm");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        match line.trim().strip_prefix("; inherits:") {
            Some(parents) => {
                for parent in parents.split(',').map(str::trim) {
                    if queries.join(parent).join("injections.scm").exists() {
                        out.push_str(&inherited(queries, parent, depth + 1)?);
                        out.push('\n');
                    }
                }
            }
            None => out.push_str(line),
        }
    }
    Ok(out)
}

/// A piece of a query file: what lies between patterns, or a pattern.
#[derive(Debug, PartialEq)]
enum Item<'a> {
    Between(&'a str),
    Pattern(&'a str),
}

impl Item<'_> {
    fn text(&self) -> &str {
        match self {
            Self::Between(text) | Self::Pattern(text) => text,
        }
    }
}

/// A query file split into its top-level patterns and what lies between
/// them (comments, blank lines). A pattern runs from its opening bracket to
/// the one that closes it, and takes the captures and quantifiers after it.
fn items(text: &str) -> Vec<Item<'_>> {
    let bytes = text.as_bytes();
    let mut items = Vec::new();
    let (mut i, mut start) = (0, 0);
    while i < bytes.len() {
        match bytes[i] {
            b';' => i = line_end(bytes, i),
            b'(' | b'[' => {
                if start < i {
                    items.push(Item::Between(&text[start..i]));
                }
                let pattern = i;
                i = group_end(bytes, i);
                // Captures and quantifiers after the group belong to it.
                loop {
                    let mut j = i;
                    while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                        j += 1;
                    }
                    match bytes.get(j) {
                        Some(b'@') => {
                            j += 1;
                            while j < bytes.len() && is_name(bytes[j]) {
                                j += 1;
                            }
                            i = j;
                        }
                        Some(b'?' | b'*' | b'+') => i = j + 1,
                        _ => break,
                    }
                }
                // The rest of its line, when it is blank.
                if text[i..].starts_with('\n') {
                    i += 1;
                }
                items.push(Item::Pattern(&text[pattern..i]));
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < text.len() {
        items.push(Item::Between(&text[start..]));
    }
    items
}

fn is_name(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')
}

fn line_end(bytes: &[u8], i: usize) -> usize {
    bytes[i..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(bytes.len(), |n| i + n)
}

/// Past the bracket that closes the one at `i`, strings and comments
/// skipped.
fn group_end(bytes: &[u8], mut i: usize) -> usize {
    let mut depth = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            b'"' => i = string_end(bytes, i) - 1,
            b';' => i = line_end(bytes, i),
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

/// Past the quote that closes the string at `i`.
fn string_end(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'"' => return i + 1,
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

/// The pattern with its strings and comments blanked, so a `#` or a bracket
/// in them is not read as code.
fn code_only(pattern: &str) -> String {
    let bytes = pattern.as_bytes();
    let mut out = String::with_capacity(pattern.len());
    let mut i = 0;
    while i < bytes.len() {
        let end = match bytes[i] {
            b'"' => string_end(bytes, i),
            b';' => line_end(bytes, i),
            _ => {
                out.push(char::from(bytes[i]));
                i += 1;
                continue;
            }
        };
        out.push_str(&" ".repeat(end - i));
        i = end;
    }
    out
}

/// The pattern without its editor-only directives.
fn strip_editor_only(pattern: &str) -> String {
    let mut pattern = pattern.to_owned();
    for directive in EDITOR_ONLY {
        while let Some(at) = code_only(&pattern).find(&format!("(#{directive}")) {
            let end = group_end(pattern.as_bytes(), at);
            pattern.replace_range(at..end, "");
        }
    }
    pattern
}

/// Why `pattern` is left out; `None` when zoll keeps it.
fn left_out(pattern: &str, grammar: &tree_sitter::Language) -> Option<String> {
    let code = code_only(pattern);
    for (at, _) in code.match_indices('#') {
        let name: String = code[at + 1..]
            .chars()
            .take_while(|&c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '?' | '!'))
            .collect();
        if !SUPPORTED.contains(&name.as_str()) {
            return Some(format!("#{name} is not evaluated"));
        }
    }
    if let Some(language) = static_language(pattern)
        && !LANGUAGES.contains(&from_helix(&language))
    {
        return Some(format!("zoll has no {language}"));
    }
    Query::new(grammar, pattern).err().map(|e| {
        format!(
            "does not compile: {}",
            e.message.lines().next().unwrap_or_default().trim()
        )
    })
}

/// The language a pattern names with `#set! injection.language`.
fn static_language(pattern: &str) -> Option<String> {
    let at = pattern.find("injection.language \"")? + "injection.language \"".len();
    let end = pattern[at..].find('"')?;
    Some(pattern[at..at + end].to_owned())
}

/// The kept text, without the blank lines what was left out leaves behind.
fn tidy(text: &str) -> String {
    let mut out = String::new();
    let mut blank = true;
    for line in text.lines() {
        let is_blank = line.trim().is_empty();
        if !(is_blank && blank) {
            out.push_str(line.trim_end());
            out.push('\n');
        }
        blank = is_blank;
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// `queries/languages.toml`: each Helix language and the regex its
/// injection name is matched with.
fn languages(helix: &Path) -> Result<String> {
    let path = helix.join("languages.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let table: toml::Table = toml::from_str(&text).map_err(|e| format!("languages.toml: {e}"))?;
    let mut regexes = BTreeMap::new();
    for language in table
        .get("language")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = language.get("name").and_then(toml::Value::as_str);
        let regex = language
            .get("injection-regex")
            .and_then(toml::Value::as_str);
        if let (Some(name), Some(regex)) = (name, regex) {
            regexes.insert(name.to_owned(), regex.to_owned());
        }
    }
    let mut out = format!(
        "# Copied by `make queries` from Helix's languages.toml at\n\
         # {HELIX}: each language Helix knows, and the\n\
         # regex the name of an injection's language is matched with, whole.\n#\n{}\n\n\
         [injection-regex]\n",
        MPL.replace(';', "#")
    );
    for (name, regex) in regexes {
        let _ = writeln!(
            out,
            "{} = {}",
            toml::Value::String(name),
            toml::Value::String(regex)
        );
    }
    Ok(out)
}

fn write(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn remove(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(format!("{}: {e}", path.display())),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_take_their_captures() {
        let text = "; a comment (not a pattern)\n((comment) @x\n (#set! injection.language \"comment\"))\n\n(a (b) @c)+ @d\n";
        assert_eq!(
            items(text),
            vec![
                Item::Between("; a comment (not a pattern)\n"),
                Item::Pattern("((comment) @x\n (#set! injection.language \"comment\"))\n"),
                Item::Between("\n"),
                Item::Pattern("(a (b) @c)+ @d\n"),
            ]
        );
    }

    #[test]
    fn a_bracket_in_a_string_is_text() {
        let text = "((x) @y (#match? @y \"^(a|b\"))\n";
        assert_eq!(items(text), vec![Item::Pattern(text)]);
    }

    #[test]
    fn editor_only_directives_go() {
        assert_eq!(
            strip_editor_only("((x) @c (#offset! @c 0 1 0 -1) (#set! injection.language \"a\"))"),
            "((x) @c  (#set! injection.language \"a\"))"
        );
    }
}
