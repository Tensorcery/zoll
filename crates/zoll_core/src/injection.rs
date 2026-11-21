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
use crate::decode::{self, Map, Part};
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
    // A comment that names the language (`/* bash */`, `# bash`) names it
    // without its markers.
    let name = name
        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '/' | '*' | '#' | '-' | ';'))
        .to_ascii_lowercase();
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
    /// The content's ranges in the host, each with its node's type: two
    /// matches with the same are of one node.
    ranges: Vec<(Range<usize>, &'a str)>,
    /// The content, decoded.
    parts: Vec<Part>,
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
        text,
        matched(language, injections, content, text, tree),
    ))
}

/// Every match of the query, with its language and its ranges; one per
/// node when two patterns match it.
fn matched<'a>(
    host: &str,
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
        let nodes: Vec<Node<'a>> = m
            .captures()
            .iter()
            .filter(|c| c.index == content)
            .map(|c| c.node)
            .collect();
        let Some(&first) = nodes.first() else {
            continue;
        };
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
        // A piece of a string literal (between two escapes, around an
        // interpolation) stands for the whole literal, decoded.
        let mut nodes: Vec<Node<'a>> = nodes
            .into_iter()
            .map(|node| {
                node.parent()
                    .filter(|p| decode::is_string(host, p.kind()))
                    .unwrap_or(node)
            })
            .collect();
        nodes.dedup_by_key(|n| n.id());
        let mut ranges = Vec::new();
        let mut parts = Vec::new();
        for node in &nodes {
            if let Some(string) = decode::string(host, *node, text) {
                ranges.push((node.byte_range(), node.kind()));
                parts.extend(string);
                continue;
            }
            for range in node_ranges(*node, properties) {
                parts.extend(decode::node(host, node.kind(), text, range.clone()));
                ranges.push((range, node.kind()));
            }
        }
        let first = nodes.first().copied().unwrap_or(first);
        let found_here = Found {
            pattern: m.pattern_index,
            combined: properties.combined,
            language: target,
            guessed,
            ranges,
            parts,
            node: first.byte_range(),
            kind: first.kind(),
        };
        // Two patterns on one node: the later one, as a query that inherits
        // another overrides it (C++ over C), unless it only guesses the
        // language (a shebang) and the earlier one names it.
        match by_ranges.get(&found_here.ranges) {
            Some(&i) if found[i].guessed || !found_here.guessed => found[i] = found_here,
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
fn node_ranges(node: Node<'_>, properties: &Properties) -> Vec<Range<usize>> {
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
fn join(text: &str, found: Vec<Found<'_>>) -> Vec<Fragment> {
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
        let fragment = &mut fragments[i];
        decode::append(text, &f.parts, &mut fragment.text, &mut fragment.map);
    }
    fragments.retain(|f| !f.text.trim().is_empty());
    fragments
}

/// Between two matches of a combined pattern: a line break when a line
/// ends between them in the host, nothing when it is continued (`\` at its
/// end).
fn separate(text: &str, fragment: &mut Fragment) {
    let Some(end) = fragment.map.end() else {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        // Nix names a string's language in a comment before it.
        assert_eq!(resolve("/* bash */"), Some(Target::Zoll("bash")));
        assert_eq!(resolve("# python"), Some(Target::Zoll("python")));
    }
}
