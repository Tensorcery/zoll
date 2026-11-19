//! What a policy is given for a file: `input`, and the facts zoll and
//! conftest put under `data` (docs/design.md, sections 8.1 and 8.3).

use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json};

use crate::document;
use crate::error::Error;
use crate::walk::File;

/// One evaluation of the policies: a document and what is known of it.
#[derive(Debug, Serialize)]
pub struct Evaluation {
    /// The document, `input` to the policy.
    pub input: Value,
    /// `data.conftest.file` and `data.zoll.file`.
    pub data: Value,
}

/// The evaluations of a file: one per document.
///
/// # Errors
///
/// The file cannot be read or does not parse, or is code, which zoll does
/// not parse yet.
pub fn evaluations(file: &File) -> Result<Vec<Evaluation>, Error> {
    let text = std::fs::read_to_string(&file.path).map_err(|e| Error::Io(file.path.clone(), e))?;
    let Some(documents) = document::documents(file.language, &file.path, &text)? else {
        return Err(Error::Other(format!(
            "{}: parsing {} is not built yet; zoll reads YAML, JSON and TOML",
            file.relative, file.language
        )));
    };
    let data = data(file);
    Ok(documents
        .into_iter()
        .map(|input| Evaluation {
            input,
            data: data.clone(),
        })
        .collect())
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
