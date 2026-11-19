//! Data formats into the documents conftest gives a policy
//! (docs/design.md, section 8.1).

mod yaml;

use std::path::Path;

use serde_json::{Map, Number, Value};

use crate::error::{Error, SourceError, char_span};

/// The documents of a data file, each evaluated on its own: the documents of
/// a YAML stream, and the items of a root array, as conftest evaluates them.
/// `None` when the language is not a data format.
///
/// # Errors
///
/// The text does not parse, or holds a value JSON has no room for.
pub fn documents(language: &str, path: &Path, text: &str) -> Result<Option<Vec<Value>>, Error> {
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
            let json = text.strip_prefix('\u{feff}').unwrap_or(text);
            let document = serde_json::from_str(json).map_err(|e| {
                let offset = text.len() - json.len();
                // The place is drawn under the message; it need not say it too.
                let message = e.to_string();
                let message = message.split(" at line ").next().unwrap_or(&message);
                let span = at_line(json, e.line(), e.column(), offset);
                failed(message.to_owned(), Some(span))
            })?;
            vec![document]
        }
        "toml" => {
            let table: toml::Table =
                toml::from_str(text).map_err(|e| failed(e.message().to_owned(), e.span()))?;
            vec![from_toml(toml::Value::Table(table)).map_err(|e| failed(e, None))?]
        }
        _ => return Ok(None),
    };
    Ok(Some(match <[Value; 1]>::try_from(documents) {
        // One document whose root is an array: each item on its own.
        Ok([Value::Array(items)]) => items,
        Ok([document]) => vec![document],
        // No document at all is one document, null, as conftest has it.
        Err(documents) if documents.is_empty() => vec![Value::Null],
        Err(documents) => documents,
    }))
}

/// The byte range of the character at `line` and `column` (both from 1, the
/// column in bytes, as `serde_json` counts), `offset` bytes into the file.
fn at_line(text: &str, line: usize, column: usize, offset: usize) -> std::ops::Range<usize> {
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
