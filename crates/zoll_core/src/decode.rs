//! A fragment's text as its interpreter receives it, decoded from the host,
//! and the map from its bytes back to the host's (docs/design.md, section
//! 14): YAML scalars as YAML reads them, string literals with their escapes
//! resolved.

use std::ops::Range;

use tree_sitter::Node;

/// What an interpolation (`#{x}`, `${x}`, `$x`) stands for: its value is
/// not known before the program runs, so a plain word takes its place.
pub(crate) const UNKNOWN: &str = "zoll";

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

    pub(crate) fn push(&mut self, from: usize, len: usize, to: Range<usize>) {
        self.0.push(Run { from, len, to });
    }

    /// Where the last run ends in the host.
    pub(crate) fn end(&self) -> Option<usize> {
        self.0.last().map(|r| r.to.end)
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

/// A piece of decoded text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Part {
    /// The host's bytes, as they are.
    Copied(Range<usize>),
    /// Text that stands for these bytes of the host.
    Put(String, Range<usize>),
}

impl Part {
    fn put(text: &str, range: Range<usize>) -> Self {
        Self::Put(text.to_owned(), range)
    }
}

/// Appends `parts` of `host` to `text`, and their places to `map`.
pub(crate) fn append(host: &str, parts: &[Part], text: &mut String, map: &mut Map) {
    for part in parts {
        let from = text.len();
        match part {
            Part::Copied(range) => {
                text.push_str(&host[range.clone()]);
                map.push(from, range.len(), range.clone());
            }
            Part::Put(put, range) => {
                text.push_str(put);
                map.push(from, put.len(), range.clone());
            }
        }
    }
}

/// The parts of `range`, a node of `kind` in a file of language `host`.
pub(crate) fn node(host: &str, kind: &str, text: &str, range: Range<usize>) -> Vec<Part> {
    match (host, kind) {
        ("yaml", "block_scalar") => block_scalar(text, range),
        ("yaml", "plain_scalar" | "string_scalar") => flow_scalar(text, range, Quote::None),
        ("yaml", "single_quote_scalar") => flow_scalar(text, inner(range), Quote::Single),
        ("yaml", "double_quote_scalar") => flow_scalar(text, inner(range), Quote::Double),
        // A shell string's quotes are not the program's.
        ("bash", "raw_string") => vec![Part::Copied(inner(range))],
        // `//go:generate cmd args`: the command is what follows.
        ("go", "comment") => {
            let rest = text[range.clone()]
                .strip_prefix("//go:generate")
                .map_or(0, |rest| {
                    range.len() - rest.trim_start_matches([' ', '\t']).len()
                });
            vec![Part::Copied(range.start + rest..range.end)]
        }
        _ => vec![Part::Copied(range)],
    }
}

/// A quoted node without its quotes.
fn inner(range: Range<usize>) -> Range<usize> {
    if range.len() >= 2 {
        range.start + 1..range.end - 1
    } else {
        range
    }
}

/// The lines of `range`, each with where it starts, line breaks included.
fn lines(text: &str, range: Range<usize>) -> impl Iterator<Item = (usize, &str)> {
    let start = range.start;
    text[range].split_inclusive('\n').scan(start, |at, line| {
        let here = *at;
        *at += line.len();
        Some((here, line))
    })
}

/// How far into its line byte `at` is, in bytes.
fn column(text: &str, at: usize) -> usize {
    at - text[..at].rfind('\n').map_or(0, |i| i + 1)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

/// A flow scalar's value (plain, single- or double-quoted, `range` inside
/// its quotes): line breaks folded as YAML folds them, a single break a
/// space and each empty line a break, the white space around a break gone;
/// `''` a quote in single quotes, the escapes resolved in double quotes.
fn flow_scalar(text: &str, range: Range<usize>, quote: Quote) -> Vec<Part> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let mut copied: Option<usize> = None;
    let mut i = range.start;
    let flush = |parts: &mut Vec<Part>, copied: &mut Option<usize>, end: usize| {
        if let Some(start) = copied.take()
            && start < end
        {
            parts.push(Part::Copied(start..end));
        }
    };
    while i < range.end {
        let b = bytes[i];
        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
            // White space up to the next text: kept unless a break is in it.
            let start = i;
            while i < range.end && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
                i += 1;
            }
            let breaks = text[start..i].matches('\n').count();
            if breaks == 0 {
                copied.get_or_insert(start);
                continue;
            }
            flush(&mut parts, &mut copied, start);
            // White space before the first text and after the last is the
            // node's edge, not the value's: a plain scalar's node has none.
            let fold = if breaks == 1 {
                " ".to_owned()
            } else {
                "\n".repeat(breaks - 1)
            };
            parts.push(Part::Put(fold, start..i));
            continue;
        }
        if quote == Quote::Single && b == b'\'' && bytes.get(i + 1) == Some(&b'\'') {
            flush(&mut parts, &mut copied, i);
            parts.push(Part::put("'", i..i + 2));
            i += 2;
            continue;
        }
        if quote == Quote::Double && b == b'\\' {
            flush(&mut parts, &mut copied, i);
            i = escape(text, i, range.end, &mut parts);
            continue;
        }
        copied.get_or_insert(i);
        i += text[i..].chars().next().map_or(1, char::len_utf8);
    }
    flush(&mut parts, &mut copied, range.end);
    parts
}

/// The escape at `at` of a double-quoted YAML scalar, as a part; where the
/// text after it starts.
fn escape(text: &str, at: usize, end: usize, parts: &mut Vec<Part>) -> usize {
    let rest = &text[at + 1..end];
    let Some(c) = rest.chars().next() else {
        parts.push(Part::Copied(at..end));
        return end;
    };
    // An escaped line break joins the lines with nothing between them.
    if c == '\n' || rest.starts_with("\r\n") {
        let after = at + 1 + if c == '\n' { 1 } else { 2 };
        let skip = text[after..end]
            .find(|c: char| c != ' ' && c != '\t')
            .map_or(end, |i| after + i);
        parts.push(Part::put("", at..skip));
        return skip;
    }
    let hex = |len: usize| {
        rest.get(1..=len)
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .and_then(char::from_u32)
            .map(|c| (c.to_string(), 1 + len))
    };
    let simple = |s: &str| Some((s.to_owned(), 1));
    let decoded = match c {
        '0' => simple("\0"),
        'a' => simple("\u{7}"),
        'b' => simple("\u{8}"),
        't' | '\t' => simple("\t"),
        'n' => simple("\n"),
        'v' => simple("\u{b}"),
        'f' => simple("\u{c}"),
        'r' => simple("\r"),
        'e' => simple("\u{1b}"),
        ' ' => simple(" "),
        '"' => simple("\""),
        '/' => simple("/"),
        '\\' => simple("\\"),
        'N' => simple("\u{85}"),
        '_' => simple("\u{a0}"),
        'L' => simple("\u{2028}"),
        'P' => simple("\u{2029}"),
        'x' => hex(2),
        'u' => hex(4),
        'U' => hex(8),
        _ => None,
    };
    if let Some((value, len)) = decoded {
        parts.push(Part::Put(value, at..at + 1 + len));
        at + 1 + len
    } else {
        // Not an escape YAML has: the parser refuses the file before this.
        let len = 1 + c.len_utf8();
        parts.push(Part::Copied(at..at + len));
        at + len
    }
}

/// A line of a block scalar.
enum Line {
    /// Text, without the block's indentation; `spaced` when it is indented
    /// further, which `>` keeps from folding.
    Text { range: Range<usize>, spaced: bool },
    /// An empty line.
    Empty,
}

/// A block scalar's value (`range` from its header): its lines without the
/// block's indentation, folded for `>` as YAML folds them, the end chomped
/// as `-` (strip), `+` (keep) or neither (clip) says.
fn block_scalar(text: &str, range: Range<usize>) -> Vec<Part> {
    let (folded, chomp, block) = block_lines(text, range);
    fold(folded, chomp, &block)
}

/// A block scalar's header, `>` and its chomping, and its lines, each with
/// its line break.
fn block_lines(text: &str, range: Range<usize>) -> (bool, char, Vec<(Line, Range<usize>)>) {
    let header_end = text[range.start..]
        .find('\n')
        .map_or(text.len(), |i| range.start + i + 1);
    let header = &text[range.start..header_end];
    let folded = header.starts_with('>');
    let indicators: String = header
        .chars()
        .skip(1)
        .take_while(|c| !c.is_whitespace() && *c != '#')
        .collect();
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
    // The lines after the header that are the block's: empty, or indented
    // as far as its first line.
    let indentation = |line: &str| line.len() - line.trim_start_matches(' ').len();
    let blank = |line: &str| {
        line.trim_end_matches(['\n', '\r'])
            .trim_start_matches(' ')
            .is_empty()
    };
    let after: Vec<(usize, &str)> = lines(text, header_end..text.len()).collect();
    let indent = explicit.map_or_else(
        || {
            after
                .iter()
                .find(|(_, l)| !blank(l))
                .map_or(usize::MAX, |(_, l)| indentation(l))
        },
        |d| parent_indent(text, range.start) + d,
    );
    let parent = parent_indent(text, range.start);
    let mut block: Vec<(Line, Range<usize>)> = Vec::new();
    for (at, line) in after {
        let body = line.trim_end_matches(['\n', '\r']);
        let break_range = at + body.len()..at + line.len();
        if blank(line) && indentation(line) <= indent {
            block.push((Line::Empty, break_range));
            continue;
        }
        if indentation(line) < indent || indent <= parent && explicit.is_none() {
            break;
        }
        let start = at + indent;
        let spaced = text[start..].starts_with([' ', '\t']);
        block.push((
            Line::Text {
                range: start..at + body.len(),
                spaced,
            },
            break_range,
        ));
    }
    (folded, chomp, block)
}

/// The value of a block scalar's lines: their breaks folded for `>`, its end
/// chomped.
fn fold(folded: bool, chomp: char, block: &[(Line, Range<usize>)]) -> Vec<Part> {
    // Empty lines after the last text: trailing, chomped.
    let last_text = block
        .iter()
        .rposition(|(l, _)| matches!(l, Line::Text { .. }));
    let mut parts = Vec::new();
    let Some(last_text) = last_text else {
        if chomp == '+' {
            for (_, br) in block {
                parts.push(Part::Put("\n".to_owned(), br.clone()));
            }
        }
        return parts;
    };
    let mut previous: Option<(bool, Range<usize>)> = None;
    let mut empties: Vec<Range<usize>> = Vec::new();
    for (line, br) in &block[..=last_text] {
        match line {
            Line::Empty => empties.push(br.clone()),
            Line::Text { range, spaced } => {
                let breaks = empties.len();
                match &previous {
                    None => {
                        // Empty lines before the first text are kept.
                        for empty in empties.drain(..) {
                            parts.push(Part::put("\n", empty));
                        }
                    }
                    Some((previous_spaced, previous_break)) => {
                        let joined = folded && !previous_spaced && !spaced;
                        let separator = match (joined, breaks) {
                            (true, 0) => " ".to_owned(),
                            (true, n) => "\n".repeat(n),
                            (false, n) => "\n".repeat(n + 1),
                        };
                        parts.push(Part::Put(separator, previous_break.start..range.start));
                        empties.clear();
                    }
                }
                parts.push(Part::Copied(range.clone()));
                previous = Some((*spaced, br.clone()));
            }
        }
    }
    let final_break = previous.map_or(0..0, |(_, br)| br);
    match chomp {
        '-' => {}
        '+' => {
            parts.push(Part::put("\n", final_break));
            for (_, br) in &block[last_text + 1..] {
                parts.push(Part::put("\n", br.clone()));
            }
        }
        _ => parts.push(Part::put("\n", final_break)),
    }
    parts
}

/// The indentation of the node a block scalar's header belongs to: the
/// column its key starts at, past the `- ` of sequence entries before it.
fn parent_indent(text: &str, header: usize) -> usize {
    let line_start = header - column(text, header);
    let line = &text[line_start..header];
    let mut at = 0;
    loop {
        at += line[at..].len() - line[at..].trim_start_matches(' ').len();
        if line[at..].starts_with("- ") {
            at += 1;
        } else {
            break;
        }
    }
    if line[at..].trim().is_empty() || line[at..].starts_with(['|', '>']) {
        // The scalar is the node itself (`- |`, or a header on its own
        // line): its parent is the sequence or mapping before it.
        at.saturating_sub(2)
    } else {
        at
    }
}

/// The string literal `node` of a file of language `host`, decoded: its
/// content copied, its escapes resolved, each interpolation [`UNKNOWN`];
/// `None` when `node` is not a string literal of `host`.
pub(crate) fn string(host: &str, node: Node<'_>, text: &str) -> Option<Vec<Part>> {
    if !is_string(host, node.kind()) {
        return None;
    }
    // A Python string's prefix: `r` keeps every backslash.
    let raw = node
        .child(0)
        .filter(|c| c.kind() == "string_start")
        .is_some_and(|c| text[c.byte_range()].contains(['r', 'R']));
    let mut parts = Vec::new();
    pieces(host, node, raw, text, true, &mut parts);
    Some(parts)
}

/// The parts of `node`, a string literal or a piece of one: the text
/// between its children as content, its escapes resolved, its content
/// pieces taken apart in turn (Python's hold the escapes), any other child
/// an interpolation. The literal's own quotes are not its value.
fn pieces(host: &str, node: Node<'_>, raw: bool, text: &str, literal: bool, parts: &mut Vec<Part>) {
    let mut at = node.start_byte();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // Before the first child of a literal is its opening quote.
        if at < child.start_byte() && !(literal && at == node.start_byte()) {
            content(host, text, at..child.start_byte(), parts);
        }
        at = child.end_byte();
        // A literal's unnamed children are its quotes; a piece's are text
        // (Python's `\.`, an escape Python does not have).
        if !child.is_named() && !literal {
            content(host, text, child.byte_range(), parts);
            continue;
        }
        if !child.is_named() || matches!(child.kind(), "string_start" | "string_end") {
            continue;
        }
        match child.kind() {
            "string_content" | "string_fragment" | "interpreted_string_literal_content" => {
                pieces(host, child, raw, text, false, parts);
            }
            "escape_sequence" if raw => parts.push(Part::Copied(child.byte_range())),
            "escape_sequence" => parts.push(code_escape(host, text, child.byte_range())),
            _ => parts.push(Part::put(UNKNOWN, child.byte_range())),
        }
    }
    if !literal && at < node.end_byte() {
        content(host, text, at..node.end_byte(), parts);
    }
}

/// Whether `kind` is a string literal of `host` that may hold escapes or
/// interpolations among its parts.
pub(crate) fn is_string(host: &str, kind: &str) -> bool {
    matches!(
        (host, kind),
        ("rust", "string_literal")
            | ("python" | "ruby" | "bash", "string")
            | ("go", "interpreted_string_literal")
            | (
                "javascript" | "typescript" | "tsx",
                "string" | "template_string"
            )
    )
}

/// A string's content: in bash's double quotes a backslash before `$`, `` ` ``,
/// `"`, `\` or a line break is the shell's, not the program's.
fn content(host: &str, text: &str, range: Range<usize>, parts: &mut Vec<Part>) {
    if host != "bash" {
        parts.push(Part::Copied(range));
        return;
    }
    let bytes = text.as_bytes();
    let mut start = range.start;
    let mut i = range.start;
    while i + 1 < range.end {
        if bytes[i] == b'\\' && matches!(bytes[i + 1], b'$' | b'`' | b'"' | b'\\' | b'\n') {
            if start < i {
                parts.push(Part::Copied(start..i));
            }
            let kept = if bytes[i + 1] == b'\n' {
                String::new()
            } else {
                char::from(bytes[i + 1]).to_string()
            };
            parts.push(Part::Put(kept, i..i + 2));
            i += 2;
            start = i;
        } else {
            i += 1;
        }
    }
    if start < range.end {
        parts.push(Part::Copied(start..range.end));
    }
}

/// An escape of a string literal in `host`, resolved. One the language does
/// not have keeps its backslash in Python, as Python keeps it (`"\d"`), and
/// loses it elsewhere.
fn code_escape(host: &str, text: &str, range: Range<usize>) -> Part {
    let escape = &text[range.clone()];
    let body = &escape[1.min(escape.len())..];
    let number = |digits: &str, radix| {
        u32::from_str_radix(digits, radix)
            .ok()
            .and_then(char::from_u32)
            .map(String::from)
    };
    let decoded = match body.chars().next() {
        Some('n') => Some("\n".to_owned()),
        Some('t') => Some("\t".to_owned()),
        Some('r') => Some("\r".to_owned()),
        Some('0') if body.len() == 1 => Some("\0".to_owned()),
        Some('\\') => Some("\\".to_owned()),
        Some('"') => Some("\"".to_owned()),
        Some('\'') => Some("'".to_owned()),
        Some('`') => Some("`".to_owned()),
        Some('a') => Some("\u{7}".to_owned()),
        Some('b') => Some("\u{8}".to_owned()),
        Some('f') => Some("\u{c}".to_owned()),
        Some('v') => Some("\u{b}".to_owned()),
        Some('e') if host == "ruby" => Some("\u{1b}".to_owned()),
        Some('\n') => Some(String::new()),
        Some('u') if body.starts_with("u{") => {
            number(body.trim_start_matches("u{").trim_end_matches('}'), 16)
        }
        Some('x' | 'u' | 'U') => number(&body[1..], 16),
        Some('0'..='7') => number(body, 8),
        _ => None,
    };
    match decoded {
        Some(decoded) => Part::Put(decoded, range),
        None if host == "python" => Part::Copied(range),
        None => Part::Put(body.to_owned(), range),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn decoded(parts: &[Part], text: &str) -> (String, Map) {
        let mut out = String::new();
        let mut map = Map::default();
        append(text, parts, &mut out, &mut map);
        (out, map)
    }

    /// The value of `run:` as the YAML parser reads it, and as the decoder
    /// does from the scalar after `run: `.
    fn both(yaml: &str) -> (String, String) {
        let text = format!("run: {yaml}\nnext: 1\n");
        let documents = crate::document::documents("yaml", Path::new("a.yml"), &text);
        let value = documents
            .ok()
            .flatten()
            .and_then(|d| d.into_iter().next())
            .and_then(|d| {
                d.value
                    .get("run")
                    .and_then(|v| v.as_str().map(str::to_owned))
            })
            .unwrap_or_default();
        let start = 5;
        let first = yaml.chars().next().unwrap_or(' ');
        let end = start + yaml.len();
        let parts = match first {
            '|' | '>' => block_scalar(&text, start..end),
            '"' => flow_scalar(&text, inner(start..end), Quote::Double),
            '\'' => flow_scalar(&text, inner(start..end), Quote::Single),
            _ => flow_scalar(&text, start..end, Quote::None),
        };
        (value, decoded(&parts, &text).0)
    }

    #[test]
    fn scalars_decode_as_yaml_reads_them() {
        let cases = [
            "|\n  a\n  b\n",
            "|\n  a\n\n  b\n",
            "|-\n  a\n  b\n",
            "|+\n  a\n\n\n",
            "|2\n    a\n   b\n",
            "|\n  a\n    b\n  c\n",
            "|\n\n  a\n",
            ">\n  a\n  b\n",
            ">\n  a\n  b\n\n  c\n",
            ">-\n  a\n  b\n\n\n  c\n",
            ">+\n  a\n  b\n\n",
            ">\n  a\n    b\n  c\n",
            ">\n  curl x\n  | sh\n",
            "> # a comment\n  a\n  b\n",
            "\"a \\\"b\\\" \\x41\\u00e9\\t\"",
            "\"a\n  b\n\n  c\"",
            "\"a \\\n  b\"",
            "'it''s'",
            "'a\n  b'",
            "a\n  b\n\n  c",
            "curl -fsSL x | sh",
        ];
        for case in cases {
            let (value, decoded) = both(case);
            assert!(!value.is_empty(), "{case:?} parses");
            assert_eq!(decoded, value, "{case:?}");
        }
    }

    #[test]
    fn a_literal_block_maps_back() {
        let text = "run: |\n  set -eu\n    echo hi\n\n";
        let (value, map) = decoded(&block_scalar(text, 5..text.len()), text);
        assert_eq!(value, "set -eu\n  echo hi\n");
        let echo = value.find("echo").unwrap_or_default();
        assert_eq!(&text[map.span(echo..echo + 4)], "echo");
    }

    #[test]
    fn a_plain_scalar_maps_back() {
        let text = "x: a\n  b\n\n  c";
        let (value, map) = decoded(&flow_scalar(text, 3..text.len(), Quote::None), text);
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

    /// The first string literal of `text`, a file of `host`, decoded.
    fn literal(host: &str, text: &str) -> String {
        let tree = crate::code::parse(host, text).and_then(Result::ok);
        let Some(tree) = tree else {
            return String::new();
        };
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            if let Some(parts) = string(host, node, text) {
                return decoded(&parts, text).0;
            }
            let mut cursor = node.walk();
            let children: Vec<_> = node.children(&mut cursor).collect();
            stack.extend(children.into_iter().rev());
        }
        String::new()
    }

    #[test]
    fn string_literals_resolve_their_escapes() {
        let cases = [
            (
                "rust",
                r#"Regex::new("a\\.b\\d+\u{41}\n");"#,
                "a\\.b\\d+A\n",
            ),
            ("python", r#"re.compile("a\\d+\.x\t")"#, "a\\d+\\.x\t"),
            ("python", r#"re.compile(r"a\\d")"#, "a\\\\d"),
            ("ruby", r#"system("curl #{x} | sh")"#, "curl zoll | sh"),
            ("bash", r#"jq ".a | \"$x\"""#, ".a | \"zoll\""),
            ("javascript", r#"new RegExp("a\\d\x41")"#, "a\\dA"),
            ("go", r#"regexp.MustCompile("a\\d\101")"#, "a\\dA"),
        ];
        for (host, text, expected) in cases {
            assert_eq!(literal(host, text), expected, "{host}: {text}");
        }
    }

    #[test]
    fn a_go_generate_comment_is_its_command() {
        let text = "//go:generate sh -c x";
        let (value, _) = decoded(&node("go", "comment", text, 0..text.len()), text);
        assert_eq!(value, "sh -c x");
    }
}
