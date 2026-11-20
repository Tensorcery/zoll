//! The code a file holds in another language: found by the injection
//! queries of `queries/`, decoded into the text its interpreter receives,
//! and mapped back to where it sits (docs/design.md, sections 6 and 14).

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::{LazyLock, OnceLock};

use regex::Regex;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator, Tree};

use crate::code;
use crate::language::{self, LANGUAGES};

include!(concat!(env!("OUT_DIR"), "/queries.rs"));

/// The predicates and directives zoll evaluates; a query with any other
/// fails to load. tree-sitter evaluates the text predicates, `#set!` sets
/// the properties below.
pub const SUPPORTED: [&str; 11] = [
    "eq?",
    "not-eq?",
    "any-eq?",
    "any-not-eq?",
    "match?",
    "not-match?",
    "any-match?",
    "any-not-match?",
    "any-of?",
    "not-any-of?",
    "set!",
];

/// zoll's names that Helix spells otherwise.
const HELIX: [(&str, &str); 2] = [("c_sharp", "c-sharp"), ("proto", "protobuf")];

/// Helix's name of a zoll language.
#[must_use]
pub fn helix_name(language: &str) -> &str {
    HELIX
        .iter()
        .find(|(zoll, _)| *zoll == language)
        .map_or(language, |(_, helix)| helix)
}

/// zoll's name of a Helix language.
#[must_use]
pub fn from_helix(name: &str) -> &str {
    HELIX
        .iter()
        .find(|(_, helix)| *helix == name)
        .map_or(name, |(zoll, _)| zoll)
}

/// The language a fragment is code of.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Target {
    /// A language of zoll's.
    Zoll(&'static str),
    /// A language zoll has no grammar for, by Helix's name.
    Lacking(String),
}

/// Code found in a host: its text as its interpreter receives it, and where
/// each part of it is in the host.
#[derive(Debug)]
pub struct Fragment {
    /// What it is code of.
    pub language: Target,
    /// The decoded text.
    pub text: String,
    /// Its bytes to the host's.
    pub map: Map,
    /// The first node that holds it, in the host.
    pub node: Range<usize>,
    /// That node's type.
    pub kind: String,
}

/// From the bytes of a text to those of the text it was decoded from: runs
/// copied as they are, and runs that stand for another length (an escape,
/// an indentation taken away).
#[derive(Debug, Clone, Default)]
pub struct Map(Vec<Run>);

#[derive(Debug, Clone)]
struct Run {
    /// Where it starts in the decoded text.
    from: usize,
    /// Its length there.
    len: usize,
    /// What it was decoded from.
    to: Range<usize>,
}

impl Run {
    fn copied(&self) -> bool {
        self.len == self.to.len()
    }
}

impl Map {
    /// A text that is its own source.
    #[must_use]
    pub fn identity(len: usize) -> Self {
        Self(vec![Run {
            from: 0,
            len,
            to: 0..len,
        }])
    }

    fn push(&mut self, from: usize, len: usize, to: Range<usize>) {
        self.0.push(Run { from, len, to });
    }

    /// Where the decoded bytes `span` came from.
    #[must_use]
    pub fn span(&self, span: Range<usize>) -> Range<usize> {
        let start = self.point(span.start, false);
        start..self.point(span.end, true).max(start)
    }

    /// Where decoded byte `byte` came from: as the end of a span when `end`.
    fn point(&self, byte: usize, end: bool) -> usize {
        let runs = &self.0;
        let Some(last) = runs.partition_point(|r| r.from <= byte).checked_sub(1) else {
            return runs.first().map_or(0, |r| r.to.start);
        };
        // An end where a run starts belongs to the run before it.
        let i = if end && last > 0 && runs[last].from == byte {
            last - 1
        } else {
            last
        };
        let run = &runs[i];
        let offset = byte - run.from;
        if offset >= run.len {
            run.to.end
        } else if run.copied() {
            run.to.start + offset
        } else if offset == 0 && !end {
            run.to.start
        } else {
            run.to.end
        }
    }

    /// This map followed by `outer`: from these decoded bytes to the bytes
    /// `outer` maps its own to. A copied run is split where `outer`'s runs
    /// meet, so every line keeps its own place.
    #[must_use]
    pub fn then(&self, outer: &Self) -> Self {
        let mut map = Self::default();
        for run in &self.0 {
            if !run.copied() || run.len == 0 {
                map.push(run.from, run.len, outer.span(run.to.clone()));
                continue;
            }
            let mut at = run.to.start;
            while at < run.to.end {
                // Runs are sorted and touch, so their ends only grow.
                let later = outer.0.partition_point(|r| r.from + r.len <= at);
                let next = outer
                    .0
                    .get(later)
                    .map_or(run.to.end, |r| (r.from + r.len).min(run.to.end));
                map.push(
                    run.from + (at - run.to.start),
                    next - at,
                    outer.span(at..next),
                );
                at = next;
            }
        }
        map
    }
}

/// A language's injection query, compiled, with what its patterns set.
struct Injections {
    query: Query,
    content: Option<u32>,
    language: Option<u32>,
    filename: Option<u32>,
    shebang: Option<u32>,
    patterns: Vec<Properties>,
}

/// What a pattern sets with `#set!`.
#[derive(Default)]
struct Properties {
    language: Option<String>,
    combined: bool,
    children: bool,
    unnamed_children: bool,
}

static COMPILED: LazyLock<HashMap<&'static str, OnceLock<Result<Injections, String>>>> =
    LazyLock::new(|| QUERIES.iter().map(|&(l, _)| (l, OnceLock::new())).collect());

/// The query of a host language, compiled once, when first needed; `None`
/// when it has none, or zoll was built without its grammar.
fn injections(language: &str) -> Option<&'static Result<Injections, String>> {
    let cell = COMPILED.get(language)?;
    let (_, source) = QUERIES.iter().find(|(l, _)| *l == language)?;
    let (grammar, _) = code::grammar(language)?;
    Some(cell.get_or_init(|| compile(language, &grammar, source)))
}

fn compile(
    language: &str,
    grammar: &tree_sitter::Language,
    source: &str,
) -> Result<Injections, String> {
    let failed = |message: String| format!("queries/{language}/injections.scm: {message}");
    let query = Query::new(grammar, source).map_err(|e| failed(e.to_string()))?;
    let mut patterns = Vec::with_capacity(query.pattern_count());
    for i in 0..query.pattern_count() {
        if let Some(predicate) = query.general_predicates(i).first() {
            return Err(failed(format!("#{} is not evaluated", predicate.operator)));
        }
        if let Some((property, _)) = query.property_predicates(i).first() {
            return Err(failed(format!("#is? {} is not evaluated", property.key)));
        }
        let mut properties = Properties::default();
        for setting in query.property_settings(i) {
            match &*setting.key {
                "injection.language" => {
                    properties.language = setting.value.as_deref().map(str::to_owned);
                }
                "injection.combined" => properties.combined = true,
                "injection.include-children" => properties.children = true,
                "injection.include-unnamed-children" => properties.unnamed_children = true,
                // Helix's own (`injection.parent`, ...): nothing for zoll.
                _ => {}
            }
        }
        patterns.push(properties);
    }
    Ok(Injections {
        content: query.capture_index_for_name("injection.content"),
        language: query.capture_index_for_name("injection.language"),
        filename: query.capture_index_for_name("injection.filename"),
        shebang: query.capture_index_for_name("injection.shebang"),
        patterns,
        query,
    })
}

/// Helix's languages and the regex an injection's language name is matched
/// with, whole.
static NAMES: LazyLock<Vec<(String, Regex)>> = LazyLock::new(|| {
    let table: toml::Table = toml::from_str(LANGUAGE_NAMES).unwrap_or_default();
    table
        .get("injection-regex")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
        .filter_map(|(name, regex)| {
            let regex = Regex::new(&format!("^(?:{})$", regex.as_str()?)).ok()?;
            Some((name.clone(), regex))
        })
        .collect()
});

/// The language an injection names: zoll's own name, or Helix's regex of a
/// language; `None` for a name no language has (`text`, `console`).
#[must_use]
pub fn resolve(name: &str) -> Option<Target> {
    let name = name.trim().to_ascii_lowercase();
    if let Some(language) = LANGUAGES.iter().find(|l| **l == name) {
        return Some(Target::Zoll(language));
    }
    let (helix, _) = NAMES.iter().find(|(_, regex)| regex.is_match(&name))?;
    let zoll = from_helix(helix);
    Some(
        LANGUAGES
            .iter()
            .find(|l| **l == zoll)
            .map_or_else(|| Target::Lacking(helix.clone()), |l| Target::Zoll(l)),
    )
}

/// One match of a pattern, before the matches of a combined pattern are
/// joined.
struct Found<'a> {
    pattern: usize,
    combined: bool,
    language: Target,
    /// The language came from a shebang or a file name, not a name: a match
    /// that names it wins over one that guesses.
    guessed: bool,
    /// The content's ranges in the host, each with its node's type.
    ranges: Vec<(Range<usize>, &'a str)>,
    node: Range<usize>,
    kind: &'a str,
}

/// The fragments the injection query of `language` finds in `tree`, the
/// tree of `text`.
///
/// # Errors
///
/// The query does not load: a file of `queries/` zoll cannot evaluate.
pub fn fragments(language: &str, text: &str, tree: &Tree) -> Result<Vec<Fragment>, String> {
    let Some(injections) = injections(language) else {
        return Ok(Vec::new());
    };
    let injections = injections.as_ref().map_err(Clone::clone)?;
    let Some(content) = injections.content else {
        return Ok(Vec::new());
    };
    Ok(join(
        language,
        text,
        matched(injections, content, text, tree),
    ))
}

/// Every match of the query, with its language and its ranges; one per
/// node when two patterns match it.
fn matched<'a>(
    injections: &'a Injections,
    content: u32,
    text: &'a str,
    tree: &'a Tree,
) -> Vec<Found<'a>> {
    let mut found: Vec<Found<'a>> = Vec::new();
    let mut by_ranges: HashMap<Vec<(Range<usize>, &'a str)>, usize> = HashMap::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&injections.query, tree.root_node(), text.as_bytes());
    while let Some(m) = matches.next() {
        let properties = &injections.patterns[m.pattern_index];
        let nodes: Vec<Node<'_>> = m
            .captures()
            .iter()
            .filter(|c| c.index == content)
            .map(|c| c.node)
            .collect();
        let Some(first) = nodes.first() else { continue };
        let captured = |index: Option<u32>| {
            m.captures()
                .iter()
                .find(|c| Some(c.index) == index)
                .and_then(|c| text.get(c.node.byte_range()))
        };
        let (target, guessed) = if let Some(name) = &properties.language {
            (resolve(name), false)
        } else if let Some(name) = captured(injections.language) {
            (resolve(name), false)
        } else if let Some(name) = captured(injections.filename) {
            (
                language::from_name(Path::new(name.trim())).map(Target::Zoll),
                true,
            )
        } else if let Some(head) = captured(injections.shebang) {
            (
                head.lines()
                    .next()
                    .and_then(language::of_shebang)
                    .map(Target::Zoll),
                true,
            )
        } else {
            (None, false)
        };
        let Some(target) = target else { continue };
        let ranges = nodes
            .iter()
            .flat_map(|node| {
                let kind = node.kind();
                ranges(*node, properties)
                    .into_iter()
                    .map(move |range| (range, kind))
            })
            .collect();
        let found_here = Found {
            pattern: m.pattern_index,
            combined: properties.combined,
            language: target,
            guessed,
            ranges,
            node: first.byte_range(),
            kind: first.kind(),
        };
        // Two patterns on one node (a fence with a language and a shebang):
        // the one that names it.
        match by_ranges.get(&found_here.ranges) {
            Some(&i) if found[i].guessed && !found_here.guessed => found[i] = found_here,
            Some(_) => {}
            None => {
                by_ranges.insert(found_here.ranges.clone(), found.len());
                found.push(found_here);
            }
        }
    }
    found
}

/// A node's ranges: without its children, unless the pattern includes them
/// (all, or the unnamed ones), as Helix reads it.
fn ranges(node: Node<'_>, properties: &Properties) -> Vec<Range<usize>> {
    if properties.children {
        return vec![node.byte_range()];
    }
    let mut ranges = Vec::new();
    let mut start = node.start_byte();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if properties.unnamed_children && !child.is_named() {
            continue;
        }
        ranges.push(start..child.start_byte());
        start = child.end_byte();
    }
    ranges.push(start..node.end_byte());
    ranges.retain(|r| !r.is_empty());
    ranges
}

/// The fragments, the matches of a combined pattern joined into one.
fn join(host: &str, text: &str, found: Vec<Found<'_>>) -> Vec<Fragment> {
    let mut fragments: Vec<Fragment> = Vec::new();
    let mut combined: HashMap<(usize, Target), usize> = HashMap::new();
    for f in found {
        let joined = f
            .combined
            .then(|| combined.get(&(f.pattern, f.language.clone())).copied())
            .flatten();
        let i = if let Some(i) = joined {
            separate(text, &mut fragments[i]);
            i
        } else {
            if f.combined {
                combined.insert((f.pattern, f.language.clone()), fragments.len());
            }
            fragments.push(Fragment {
                language: f.language,
                text: String::new(),
                map: Map::default(),
                node: f.node,
                kind: f.kind.to_owned(),
            });
            fragments.len() - 1
        };
        for (range, kind) in f.ranges {
            decode(host, kind, text, range, &mut fragments[i]);
        }
    }
    fragments.retain(|f| !f.text.trim().is_empty());
    fragments
}

/// Between two matches of a combined pattern: a line break when a line
/// ends between them in the host, nothing when it is continued (`\` at its
/// end).
fn separate(text: &str, fragment: &mut Fragment) {
    let Some(end) = fragment.map.0.last().map(|r| r.to.end) else {
        return;
    };
    let rest = &text[end..];
    let continued = rest.trim_start_matches([' ', '\t']).starts_with("\\\n");
    if !continued {
        let from = fragment.text.len();
        fragment.text.push('\n');
        fragment.map.push(from, 1, end..end);
    }
}

/// Appends the text of `range`, a node of `kind` in `host`, decoded.
fn decode(host: &str, kind: &str, text: &str, range: Range<usize>, fragment: &mut Fragment) {
    let mut parts = Vec::new();
    match (host, kind) {
        ("yaml", "block_scalar") => block_scalar(text, range, &mut parts),
        ("yaml", "string_scalar") => plain_scalar(text, range, &mut parts),
        // A shell string's quotes are not the program's.
        ("bash", "raw_string") if range.len() >= 2 => {
            parts.push(Part::Copied(range.start + 1..range.end - 1));
        }
        _ => parts.push(Part::Copied(range)),
    }
    for part in parts {
        let from = fragment.text.len();
        match part {
            Part::Copied(range) => {
                fragment.text.push_str(&text[range.clone()]);
                fragment.map.push(from, range.len(), range);
            }
            Part::Put(put, range) => {
                fragment.text.push_str(put);
                fragment.map.push(from, put.len(), range);
            }
        }
    }
}

/// A piece of decoded text.
enum Part {
    /// The host's bytes, as they are.
    Copied(Range<usize>),
    /// Text that stands for these bytes of the host.
    Put(&'static str, Range<usize>),
}

/// The lines of `range`, each with where it starts.
fn lines(text: &str, range: Range<usize>) -> impl Iterator<Item = (usize, &str)> {
    let start = range.start;
    text[range].split_inclusive('\n').scan(start, |at, line| {
        let here = *at;
        *at += line.len();
        Some((here, line))
    })
}

/// A plain scalar's value: its lines folded, a line break a space and an
/// empty line a line break, as YAML reads it.
fn plain_scalar(text: &str, range: Range<usize>, parts: &mut Vec<Part>) {
    let mut gap: Option<(usize, bool)> = None;
    for (at, line) in lines(text, range) {
        let body = line.trim_end_matches(['\n', '\r']);
        let trimmed = body.trim_matches([' ', '\t']);
        if trimmed.is_empty() {
            if let Some((start, _)) = gap {
                parts.push(Part::Put("\n", start..at + line.len()));
                gap = Some((at + line.len(), true));
            }
            continue;
        }
        let start = at + (body.len() - body.trim_start_matches([' ', '\t']).len());
        if let Some((end, broke)) = gap
            && !broke
        {
            parts.push(Part::Put(" ", end..start));
        }
        parts.push(Part::Copied(start..start + trimmed.len()));
        gap = Some((start + trimmed.len(), false));
    }
}

/// A block scalar's value: the header line gone, each line without the
/// block's indentation, folded for `>`, its end chomped as `-` or `+` says.
fn block_scalar(text: &str, range: Range<usize>, parts: &mut Vec<Part>) {
    let mut lines = lines(text, range);
    let Some((_, header)) = lines.next() else {
        return;
    };
    let folded = header.starts_with('>');
    let indicators = header.split_whitespace().next().unwrap_or_default();
    let chomp = if indicators.contains('-') {
        '-'
    } else if indicators.contains('+') {
        '+'
    } else {
        ' '
    };
    let explicit = indicators
        .chars()
        .find_map(|c| c.to_digit(10))
        .and_then(|d| usize::try_from(d).ok());
    let lines: Vec<(usize, &str)> = lines.collect();
    // The node holds the block's text, the indentation of its first line
    // included only when it starts a line of the file.
    let indent = explicit.unwrap_or_else(|| {
        lines
            .iter()
            .find(|(_, l)| !l.trim().is_empty())
            .map_or(0, |(at, l)| {
                column(text, *at) + l.len() - l.trim_start_matches(' ').len()
            })
    });
    // For `>`: whether the line before held text, and was not indented
    // further than the block.
    let mut previous: Option<bool> = None;
    for (at, line) in lines {
        let body = line.trim_end_matches(['\n', '\r']);
        let skip = indent
            .saturating_sub(column(text, at))
            .min(body.len() - body.trim_start_matches(' ').len());
        let start = at + skip;
        let content = start..at + body.len();
        let blank = content.is_empty();
        let deeper = text[start..].starts_with([' ', '\t']);
        if folded && previous == Some(false) {
            if blank {
                // A text line's break before empty lines is dropped.
                parts.pop();
            } else if !deeper && let Some(Part::Put(_, range) | Part::Copied(range)) = parts.pop() {
                // Joined to the line before: its line break was a space.
                parts.push(Part::Put(" ", range));
            }
        }
        if !blank {
            parts.push(Part::Copied(content.clone()));
        }
        let end = at + line.len();
        parts.push(if line.ends_with('\n') && !line.ends_with("\r\n") {
            Part::Copied(end - 1..end)
        } else {
            Part::Put("\n", end..end)
        });
        previous = (!blank).then_some(deeper);
    }
    let is_break = |p: &Part| match p {
        Part::Copied(r) => text.get(r.clone()) == Some("\n"),
        Part::Put(put, _) => *put == "\n",
    };
    if chomp != '+' {
        while parts.last().is_some_and(is_break) {
            parts.pop();
        }
        if chomp == ' ' && !parts.is_empty() {
            let end = parts.last().map_or(0, |p| match p {
                Part::Copied(r) | Part::Put(_, r) => r.end,
            });
            parts.push(Part::Put("\n", end..end));
        }
    }
}

/// How far into its line byte `at` is, in bytes.
fn column(text: &str, at: usize) -> usize {
    at - text[..at].rfind('\n').map_or(0, |i| i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(
        f: fn(&str, Range<usize>, &mut Vec<Part>),
        text: &str,
        range: Range<usize>,
    ) -> (String, Map) {
        let mut parts = Vec::new();
        f(text, range, &mut parts);
        let mut fragment = Fragment {
            language: Target::Zoll("bash"),
            text: String::new(),
            map: Map::default(),
            node: 0..0,
            kind: String::new(),
        };
        for part in parts {
            let from = fragment.text.len();
            match part {
                Part::Copied(r) => {
                    fragment.text.push_str(&text[r.clone()]);
                    fragment.map.push(from, r.len(), r);
                }
                Part::Put(put, r) => {
                    fragment.text.push_str(put);
                    fragment.map.push(from, put.len(), r);
                }
            }
        }
        (fragment.text, fragment.map)
    }

    #[test]
    fn a_literal_block_loses_its_indentation() {
        let text = "run: |\n  set -eu\n    echo hi\n\n";
        let start = text.find('|').unwrap_or_default();
        let (value, map) = decoded(block_scalar, text, start..text.len() - 1);
        assert_eq!(value, "set -eu\n  echo hi\n");
        // `echo` in the value is `echo` in the file.
        let echo = value.find("echo").unwrap_or_default();
        assert_eq!(&text[map.span(echo..echo + 4)], "echo");
    }

    #[test]
    fn a_folded_block_joins_its_lines() {
        let text = "x: >-\n  a\n  b\n\n  c\n";
        let start = text.find('>').unwrap_or_default();
        let (value, _) = decoded(block_scalar, text, start..text.len());
        assert_eq!(value, "a b\nc");
    }

    #[test]
    fn a_plain_scalar_folds() {
        let text = "x: a\n  b\n\n  c";
        let (value, map) = decoded(plain_scalar, text, 3..text.len());
        assert_eq!(value, "a b\nc");
        assert_eq!(&text[map.span(2..3)], "b");
        assert_eq!(&text[map.span(4..5)], "c");
    }

    #[test]
    fn maps_compose_line_by_line() {
        // `bc` is a copied run of the inner text across two lines of the
        // outer one.
        let mut outer = Map::default();
        outer.push(0, 2, 10..12);
        outer.push(2, 2, 20..22);
        let mut inner = Map::default();
        inner.push(0, 3, 1..4);
        let map = inner.then(&outer);
        assert_eq!(map.span(0..1), 11..12);
        assert_eq!(map.span(1..2), 20..21);
        assert_eq!(map.span(0..3), 11..22);
    }

    // The base set: a build with fewer languages leaves some queries out.
    #[cfg(feature = "base")]
    #[test]
    fn every_query_loads() {
        for (language, _) in QUERIES {
            let error = injections(language).map(|r| r.as_ref().err().cloned());
            assert_eq!(error, Some(None), "queries/{language}");
        }
    }

    #[test]
    fn names_resolve_whole() {
        assert_eq!(resolve("sh"), Some(Target::Zoll("bash")));
        assert_eq!(resolve("Python"), Some(Target::Zoll("python")));
        assert_eq!(resolve("ocaml"), Some(Target::Lacking("ocaml".to_owned())));
        // `tex` is a language; `text` is not.
        assert_eq!(resolve("text"), None);
    }
}
