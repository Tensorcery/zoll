//! Data formats into the documents conftest gives a policy
//! (docs/design.md, section 8.1).

mod yaml;

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;

use serde_json::{Map, Number, Value};
use toml::de::{DeTable, DeValue};

use crate::error::{Error, SourceError, char_span};

/// One document of a file, evaluated on its own.
#[derive(Debug)]
pub struct Document {
    /// The document.
    pub value: Value,
    /// Where in the file it came from.
    pub origin: Origin,
}

/// Where in its file a document came from: which document of a YAML
/// stream, and which item of its root array when that was split.
#[derive(Debug, Clone, Copy, Default)]
pub struct Origin {
    /// The document of the stream, from 0.
    pub document: usize,
    /// The item of the root array, from 0.
    pub item: Option<usize>,
}

/// A step of a path into a document, as a policy's `at` names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Step {
    /// A key of an object.
    Key(String),
    /// An index into an array.
    Index(usize),
}

impl Step {
    /// The step as a policy writes it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::Key(key) => Value::String(key.clone()),
            Self::Index(i) => Value::from(*i),
        }
    }

    /// A step from its JSON: a string, or a whole number from 0.
    #[must_use]
    pub fn from_json(value: &Value) -> Option<Self> {
        match value {
            Value::String(key) => Some(Self::Key(key.clone())),
            Value::Number(n) => n
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .map(Self::Index),
            _ => None,
        }
    }
}

/// A place in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The line, from 1.
    pub line: usize,
    /// The column, from 1, in characters.
    pub column: usize,
    /// The bytes it covers.
    pub span: Range<usize>,
}

impl Place {
    /// The place that starts at byte `start` of `text` and ends at `end`.
    #[must_use]
    pub fn of(text: &str, span: Range<usize>) -> Self {
        let start = span.start.min(text.len());
        let before = &text[..start];
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        Self {
            line: before.matches('\n').count() + 1,
            column: text[line_start..start].chars().count() + 1,
            span: start..span.end.clamp(start, text.len()),
        }
    }
}

/// Whether `language` is read as data rather than as code.
#[must_use]
pub fn is_data(language: &str) -> bool {
    matches!(language, "yaml" | "json" | "toml")
}

/// The documents of a data file, each evaluated on its own: the documents of
/// a YAML stream, and the items of a root array, as conftest evaluates them.
/// `None` when the language is not a data format.
///
/// # Errors
///
/// The text does not parse, or holds a value JSON has no room for.
pub fn documents(language: &str, path: &Path, text: &str) -> Result<Option<Vec<Document>>, Error> {
    let failed = |message: String, span| {
        Error::Source(SourceError {
            path: path.to_owned(),
            text: text.to_owned(),
            message,
            span,
        })
    };
    let documents = match language {
        "yaml" => {
            yaml::documents(text).map_err(|e| failed(e.message, Some(char_span(text, e.index))))?
        }
        "json" => {
            // A byte order mark is skipped, as conftest skips it.
            let (json, offset) = without_bom(text);
            let document = serde_json::from_str(json).map_err(|e| {
                // The place is drawn under the message; it need not say it too.
                let message = e.to_string();
                let message = match message.split(" at line ").next().unwrap_or(&message) {
                    // In the words YAML's limit has.
                    "recursion limit exceeded" => "nested more than 128 deep",
                    message => message,
                };
                let span = at_line(json, e.line(), e.column(), offset);
                failed(message.to_owned(), Some(span))
            })?;
            vec![document]
        }
        "toml" => {
            let (body, offset) = without_bom(text);
            let table: toml::Table = toml::from_str(body).map_err(|e| {
                let span = e.span().map(|span| span.start + offset..span.end + offset);
                failed(e.message().to_owned(), span)
            })?;
            vec![from_toml(toml::Value::Table(table)).map_err(|e| failed(e, None))?]
        }
        _ => return Ok(None),
    };
    let documents = match <[Value; 1]>::try_from(documents) {
        // One document whose root is an array: each item on its own.
        Ok([Value::Array(items)]) => items
            .into_iter()
            .enumerate()
            .map(|(i, value)| {
                (
                    value,
                    Origin {
                        document: 0,
                        item: Some(i),
                    },
                )
            })
            .collect(),
        Ok([value]) => vec![(value, Origin::default())],
        // No document at all is one document, null, as conftest has it.
        Err(documents) if documents.is_empty() => vec![(Value::Null, Origin::default())],
        Err(documents) => documents
            .into_iter()
            .enumerate()
            .map(|(i, value)| {
                (
                    value,
                    Origin {
                        document: i,
                        item: None,
                    },
                )
            })
            .collect(),
    };
    Ok(Some(
        documents
            .into_iter()
            .map(|(value, origin)| Document { value, origin })
            .collect(),
    ))
}

/// Where the paths of a data file's documents are, found once for all the
/// results in it: parsing the file again for each one would grow with the
/// square of its results.
#[derive(Debug, Default)]
pub struct Index {
    /// The place of each path, by document.
    documents: Vec<HashMap<Vec<Step>, Range<usize>>>,
}

impl Index {
    /// The places of `text`, a file of `language`; none for code.
    #[must_use]
    pub fn new(language: &str, text: &str) -> Self {
        let documents = match language {
            // JSON is YAML too, so its places come from the same parser.
            "yaml" | "json" => yaml::places(&surrogates_replaced(language, text))
                .unwrap_or_default()
                .into_iter()
                .map(|places| {
                    places
                        .into_iter()
                        .map(|(path, span)| {
                            let start = char_span(text, span.start.index()).start;
                            let end = char_span(text, span.end.index()).start;
                            (path, start..end)
                        })
                        .collect()
                })
                .collect(),
            "toml" => {
                let (body, offset) = without_bom(text);
                DeTable::parse(body).ok().map_or_else(Vec::new, |table| {
                    let mut places = HashMap::new();
                    toml_places(table.get_ref(), &mut Vec::new(), &mut places);
                    for span in places.values_mut() {
                        *span = span.start + offset..span.end + offset;
                    }
                    vec![places]
                })
            }
            _ => Vec::new(),
        };
        Self { documents }
    }

    /// Where `path` is in a document of `text`: its key, or its item; the
    /// nearest of its parents that is in the file when it is not; the start
    /// of the document for the empty path.
    #[must_use]
    pub fn locate(&self, text: &str, origin: Origin, path: &[Step]) -> Place {
        let mut full: Vec<Step> = origin.item.map(Step::Index).into_iter().collect();
        full.extend_from_slice(path);
        let found = self.documents.get(origin.document).and_then(|places| {
            (0..=full.len())
                .rev()
                .find_map(|n| places.get(&full[..n]).cloned())
        });
        Place::of(
            text,
            found.unwrap_or(0..text.chars().next().map_or(0, char::len_utf8)),
        )
    }
}

/// The text after its byte order mark, if it has one, and the mark's length:
/// the parsers' places are in the text without it.
fn without_bom(text: &str) -> (&str, usize) {
    let body = text.strip_prefix('\u{feff}').unwrap_or(text);
    (body, text.len() - body.len())
}

/// JSON's `\ud83d\ude00`, which the YAML parser refuses, as `\ufffd\ufffd`:
/// as long, so every place stays where it is. A key spelled with one is
/// placed at its parent.
fn surrogates_replaced<'t>(language: &str, text: &'t str) -> Cow<'t, str> {
    if language != "json" || !text.contains("\\u") {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let surrogate = rest[1..].starts_with('u')
            && rest
                .get(2..6)
                .and_then(|hex| u16::from_str_radix(hex, 16).ok())
                .is_some_and(|unit| (0xD800..0xE000).contains(&unit));
        // The backslash and what it escapes: `\\` is not the start of `\u`.
        let taken = if surrogate {
            6
        } else {
            1 + rest[1..].chars().next().map_or(0, char::len_utf8)
        };
        out.push_str(if surrogate { "\\ufffd" } else { &rest[..taken] });
        rest = &rest[taken..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// The place of every path of a TOML table: a key for a table's entry, the
/// value for an array's item.
fn toml_places(
    value: &DeTable<'_>,
    path: &mut Vec<Step>,
    places: &mut HashMap<Vec<Step>, Range<usize>>,
) {
    if path.is_empty() {
        places.insert(Vec::new(), 0..0);
    }
    for (key, value) in value {
        path.push(Step::Key(key.get_ref().to_string()));
        places.insert(path.clone(), key.span());
        toml_value(value.get_ref(), path, places);
        path.pop();
    }
}

fn toml_value(
    value: &DeValue<'_>,
    path: &mut Vec<Step>,
    places: &mut HashMap<Vec<Step>, Range<usize>>,
) {
    match value {
        DeValue::Table(table) => toml_places(table, path, places),
        DeValue::Array(array) => {
            for (i, item) in array.iter().enumerate() {
                path.push(Step::Index(i));
                places.insert(path.clone(), item.span());
                toml_value(item.get_ref(), path, places);
                path.pop();
            }
        }
        _ => {}
    }
}

/// The document and the path of each place of a YAML stream, by the byte
/// it starts at: the key of a mapping entry, the node of a sequence item.
/// Where two start at one byte (an item and its first key), the longer path
/// is kept.
#[must_use]
pub fn yaml_paths(text: &str) -> HashMap<usize, (usize, Vec<Step>)> {
    let bytes: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let mut paths: HashMap<usize, (usize, Vec<Step>)> = HashMap::new();
    for (document, places) in yaml::places(text)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        for (path, span) in places {
            let byte = bytes.get(span.start.index()).copied().unwrap_or(text.len());
            let longer = paths
                .get(&byte)
                .is_none_or(|(_, known)| known.len() < path.len());
            if longer {
                paths.insert(byte, (document, path));
            }
        }
    }
    paths
}

/// The byte range of the character at `line` and `column` (both from 1, the
/// column in bytes, as `serde_json` counts), `offset` bytes into the file.
fn at_line(text: &str, line: usize, column: usize, offset: usize) -> Range<usize> {
    let line_start: usize = text
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::len)
        .sum();
    let mut start = (line_start + column.saturating_sub(1)).min(text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let end = text[start..]
        .chars()
        .next()
        .map_or(start, |c| start + c.len_utf8());
    offset + start..offset + end
}

/// TOML into JSON. A date or a time is the string TOML wrote it as: conftest
/// turns a local one into the machine's time zone, which no policy can rely
/// on.
fn from_toml(value: toml::Value) -> Result<Value, String> {
    Ok(match value {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::Number(i.into()),
        toml::Value::Float(f) => {
            Value::Number(Number::from_f64(f).ok_or_else(|| format!("`{f}` has no value in JSON"))?)
        }
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(items) => {
            Value::Array(items.into_iter().map(from_toml).collect::<Result<_, _>>()?)
        }
        toml::Value::Table(table) => Value::Object(
            table
                .into_iter()
                .map(|(k, v)| Ok((k, from_toml(v)?)))
                .collect::<Result<Map<_, _>, String>>()?,
        ),
    })
}
