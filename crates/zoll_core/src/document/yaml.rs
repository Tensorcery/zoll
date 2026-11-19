//! YAML into JSON values, as YAML 1.2 reads it: `on` and `yes` stay strings,
//! unlike conftest's YAML 1.1 (docs/design.md, section 8.1).

use std::collections::HashMap;
use std::iter::Peekable;

use saphyr_parser::{Event, Parser, ScalarStyle, ScanError, Span, StrInput, Tag};
use serde_json::{Map, Number, Value};

/// Nodes one file may build, the copies anchors keep and aliases make
/// counted: a few aliases of aliases can otherwise ask for more memory than
/// the machine has.
const MAX_NODES: usize = 1_000_000;

/// Nesting deeper than this is refused rather than overflowing the stack.
const MAX_DEPTH: usize = 512;

/// What is wrong, and the character (counted from 0) it is at.
pub(super) struct YamlError {
    pub(super) message: String,
    pub(super) index: usize,
}

impl From<ScanError> for YamlError {
    fn from(error: ScanError) -> Self {
        Self {
            message: error.info().to_owned(),
            index: error.marker().index(),
        }
    }
}

/// The documents of a YAML stream, in order.
pub(super) fn documents(text: &str) -> Result<Vec<Value>, YamlError> {
    let mut builder = Builder {
        events: Parser::new_from_str(text).peekable(),
        anchors: HashMap::new(),
        nodes: 0,
    };
    let mut documents = Vec::new();
    loop {
        match builder.next()? {
            (Event::StreamEnd, _) => return Ok(documents),
            (Event::DocumentStart(_), _) => {
                documents.push(builder.node(0)?);
                builder.next()?; // DocumentEnd
            }
            _ => {} // StreamStart
        }
    }
}

struct Builder<'a> {
    events: Peekable<Parser<'a, StrInput<'a>>>,
    /// The value of each anchor, and how many nodes it holds.
    anchors: HashMap<usize, (Value, usize)>,
    /// Nodes built so far, aliases counted at their full size.
    nodes: usize,
}

impl<'a> Builder<'a> {
    fn next(&mut self) -> Result<(Event<'a>, Span), YamlError> {
        match self.events.next() {
            Some(event) => Ok(event?),
            None => Ok((Event::StreamEnd, Span::default())),
        }
    }

    fn peek_ends(&mut self) -> bool {
        matches!(
            self.events.peek(),
            Some(Ok((Event::SequenceEnd | Event::MappingEnd, _)))
        )
    }

    fn peek_merge_key(&mut self) -> bool {
        matches!(
            self.events.peek(),
            Some(Ok((Event::Scalar(value, ScalarStyle::Plain, _, None), _))) if value == "<<"
        )
    }

    fn count(&mut self, nodes: usize, span: Span) -> Result<(), YamlError> {
        self.nodes += nodes;
        if self.nodes > MAX_NODES {
            return Err(error(
                span,
                "anchors and aliases make more than a million nodes",
            ));
        }
        Ok(())
    }

    fn node(&mut self, depth: usize) -> Result<Value, YamlError> {
        let start = self.nodes;
        let (event, span) = self.next()?;
        if depth > MAX_DEPTH {
            return Err(error(span, "nested more than 512 deep"));
        }
        let (value, anchor) = match event {
            Event::Alias(id) => {
                let Some((value, size)) = self.anchors.get(&id).cloned() else {
                    return Err(error(span, "an alias of no anchor"));
                };
                self.count(size, span)?;
                return Ok(value);
            }
            Event::Scalar(text, style, anchor, tag) => {
                self.count(1, span)?;
                (scalar(&text, style, tag.as_deref(), span)?, anchor)
            }
            Event::SequenceStart(anchor, _) => {
                self.count(1, span)?;
                let mut items = Vec::new();
                while !self.peek_ends() {
                    items.push(self.node(depth + 1)?);
                }
                self.next()?;
                (Value::Array(items), anchor)
            }
            Event::MappingStart(anchor, _) => {
                self.count(1, span)?;
                (Value::Object(self.mapping(depth)?), anchor)
            }
            _ => return Err(error(span, "a node was expected here")),
        };
        if anchor != 0 {
            // The anchor keeps a copy: nested anchors copy what is under
            // them again at every level, so the copy counts too.
            let size = self.nodes - start;
            self.count(size, span)?;
            self.anchors.insert(anchor, (value.clone(), size));
        }
        Ok(value)
    }

    /// A mapping's entries, after its `MappingStart`. `<<` merges the
    /// mappings it names: their keys come first, the mapping's own override
    /// them, and an earlier merged mapping wins over a later one.
    fn mapping(&mut self, depth: usize) -> Result<Map<String, Value>, YamlError> {
        let mut own = Map::new();
        let mut merged = Map::new();
        while !self.peek_ends() {
            if self.peek_merge_key() {
                let (_, span) = self.next()?;
                let sources = match self.node(depth + 1)? {
                    Value::Array(items) => items,
                    one => vec![one],
                };
                for source in sources {
                    let Value::Object(source) = source else {
                        return Err(error(span, "`<<` merges mappings only"));
                    };
                    for (key, value) in source {
                        merged.entry(key).or_insert(value);
                    }
                }
                continue;
            }
            let span = self
                .events
                .peek()
                .and_then(|e| e.as_ref().ok())
                .map(|e| e.1);
            let key = key(self.node(depth + 1)?, span.unwrap_or_default())?;
            let value = self.node(depth + 1)?;
            own.insert(key, value);
        }
        self.next()?;
        merged.extend(own);
        Ok(merged)
    }
}

/// A key as a JSON object has it: a string. Numbers and booleans are written
/// as YAML wrote them in canonical form, as conftest does.
fn key(value: Value, span: Span) -> Result<String, YamlError> {
    match value {
        Value::String(key) => Ok(key),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Null => Err(error(span, "a null key has no place in a JSON object")),
        Value::Array(_) | Value::Object(_) => Err(error(
            span,
            "a list or a mapping as a key has no place in a JSON object",
        )),
    }
}

/// A scalar's value under the YAML 1.2 core schema: a plain scalar may be
/// null, a boolean or a number; a quoted or block one is a string; a tag
/// of the core schema says the type outright.
fn scalar(
    text: &str,
    style: ScalarStyle,
    tag: Option<&Tag>,
    span: Span,
) -> Result<Value, YamlError> {
    let core = tag
        .filter(|tag| tag.is_yaml_core_schema())
        .map(|tag| tag.suffix.as_str());
    let value = match core {
        Some("str" | "binary" | "timestamp") => Some(Value::String(text.to_owned())),
        Some("null") => Some(Value::Null),
        Some("bool") => boolean(text),
        Some("int") => integer(text),
        Some("float") => float(text, span)?,
        Some(_) | None if style != ScalarStyle::Plain => Some(Value::String(text.to_owned())),
        _ => Some(plain(text, span)?),
    };
    value.ok_or_else(|| error(span, &format!("`{text}` is not of the type its tag names")))
}

fn plain(text: &str, span: Span) -> Result<Value, YamlError> {
    if matches!(text, "" | "~" | "null" | "Null" | "NULL") {
        return Ok(Value::Null);
    }
    Ok(boolean(text)
        .or_else(|| integer(text))
        .or(float(text, span)?)
        .unwrap_or_else(|| Value::String(text.to_owned())))
}

fn boolean(text: &str) -> Option<Value> {
    match text {
        "true" | "True" | "TRUE" => Some(Value::Bool(true)),
        "false" | "False" | "FALSE" => Some(Value::Bool(false)),
        _ => None,
    }
}

/// `42`, `-7`, `+3`, `0o17`, `0x1F`. Beyond 64 bits it is a float.
fn integer(text: &str) -> Option<Value> {
    let (digits, radix) = if let Some(octal) = text.strip_prefix("0o") {
        (octal, 8)
    } else if let Some(hex) = text.strip_prefix("0x") {
        (hex, 16)
    } else {
        let unsigned = text.strip_prefix(['-', '+']).unwrap_or(text);
        if unsigned.is_empty() || !unsigned.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        return text
            .parse::<i64>()
            .map(Number::from)
            .ok()
            .or_else(|| text.parse::<u64>().ok().map(Number::from))
            .or_else(|| text.parse::<f64>().ok().and_then(Number::from_f64))
            .map(Value::Number);
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    u64::from_str_radix(digits, radix)
        .ok()
        .map(|n| Value::Number(n.into()))
}

/// `1.5`, `.5`, `1e3`. `.inf` and `.nan` have no JSON value, as conftest
/// finds too.
fn float(text: &str, span: Span) -> Result<Option<Value>, YamlError> {
    let unsigned = text.strip_prefix(['-', '+']).unwrap_or(text);
    if matches!(unsigned, ".inf" | ".Inf" | ".INF") || matches!(text, ".nan" | ".NaN" | ".NAN") {
        return Err(error(span, &format!("`{text}` has no value in JSON")));
    }
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (unsigned, None),
    };
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let mantissa_ok = match mantissa.split_once('.') {
        Some((whole, fraction)) => {
            digits(whole) && digits(fraction) && !(whole.is_empty() && fraction.is_empty())
        }
        None => !mantissa.is_empty() && digits(mantissa),
    };
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['-', '+']).unwrap_or(e);
        !e.is_empty() && digits(e)
    });
    if !mantissa_ok || !exponent_ok {
        return Ok(None);
    }
    Ok(text
        .parse::<f64>()
        .ok()
        .and_then(Number::from_f64)
        .map(Value::Number))
}

fn error(span: Span, message: &str) -> YamlError {
    YamlError {
        message: message.to_owned(),
        index: span.start.index(),
    }
}
