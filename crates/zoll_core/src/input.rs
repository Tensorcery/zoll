//! What a policy is given for a file: `input`, and the facts zoll and
//! conftest put under `data` (docs/design.md, sections 8.1 and 8.3).

use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json};

use crate::code;
use crate::document::{self, Document, Origin};
use crate::error::{Error, SourceError};
use crate::walk::File;

/// One evaluation of the policies: a document and what is known of it.
#[derive(Debug, Serialize)]
pub struct Evaluation {
    /// The document, `input` to the policy.
    pub input: Value,
    /// `data.conftest.file` and `data.zoll.file`.
    pub data: Value,
    /// Where in the file the document came from.
    #[serde(skip)]
    pub origin: Origin,
}

/// The text of a file; `-` is the standard input.
///
/// # Errors
///
/// The file cannot be read, or is not UTF-8.
pub fn read(file: &File) -> Result<String, Error> {
    let failed = |e| Error::Io(file.path.clone(), e);
    if file.path.as_os_str() == "-" {
        return std::io::read_to_string(std::io::stdin()).map_err(failed);
    }
    std::fs::read_to_string(&file.path).map_err(failed)
}

/// The evaluations of a file of this text: one per document.
///
/// # Errors
///
/// The text does not parse, or is of a language zoll was built without.
pub fn evaluations(file: &File, text: &str) -> Result<Vec<Evaluation>, Error> {
    let documents = match document::documents(file.language, &file.path, text)? {
        Some(documents) => documents,
        None => vec![Document {
            value: tree(file, text)?,
            origin: Origin::default(),
        }],
    };
    let data = data(file);
    Ok(documents
        .into_iter()
        .map(|document| Evaluation {
            input: document.value,
            data: data.clone(),
            origin: document.origin,
        })
        .collect())
}

/// The tree of a code file (section 8.2).
fn tree(file: &File, text: &str) -> Result<Value, Error> {
    let Some(tree) = code::tree(file.language, text) else {
        return Err(Error::Other(format!(
            "{}: this zoll was built without {}; `zoll languages` lists what it parses",
            file.relative, file.language
        )));
    };
    tree.map_err(|message| {
        Error::Source(SourceError {
            path: file.path.clone(),
            text: text.to_owned(),
            message,
            span: None,
        })
    })
}

fn data(file: &File) -> Value {
    let name = file.path.file_name().map(|n| n.to_string_lossy());
    let dir = file.path.parent().map(Path::to_string_lossy);
    json!({
        // As conftest sets it: the name, and the absolute directory.
        "conftest": {"file": {"name": name, "dir": dir}},
        "zoll": {"file": {"path": file.relative, "language": file.language}},
    })
}
