//! zoll gives a policy the document conftest gives it. `tests/corpus/` holds
//! real files and, next to each, the document conftest parsed it into
//! (`scripts/conftest-corpus.sh` writes them).

use crate::{TestResult, zoll};
use serde_json::Value;
use std::error::Error;
use std::fs;
use std::path::Path;

/// Where zoll differs from conftest on purpose (docs/design.md, section 8.1):
/// YAML is read as YAML 1.2, so GitHub's `on:` is the key `on`; conftest's
/// YAML 1.1 makes it `true`.
const DIFFERENCES: [(&str, &str, &str); 1] = [("workflow.yml", "true", "on")];

#[test]
fn same_documents_as_conftest() -> TestResult {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut compared = 0;
    for entry in fs::read_dir(&corpus)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name.ends_with(".conftest.json") {
            continue;
        }
        let expected = conftest(&corpus, name)?;
        let actual = zoll_inputs(&corpus, name)?;
        assert_eq!(
            actual, expected,
            "{name}: zoll's documents, then conftest's"
        );
        compared += 1;
    }
    assert!(compared >= 10, "the corpus is read: {compared} files");
    Ok(())
}

/// conftest's documents for a file: a root array is evaluated element by
/// element, so it is one document per element.
fn conftest(corpus: &Path, name: &str) -> Result<Vec<Value>, Box<dyn Error>> {
    let text = fs::read_to_string(corpus.join(format!("{name}.conftest.json")))?;
    let mut document: Value = serde_json::from_str(&text)?;
    for (file, from, to) in DIFFERENCES {
        if file == name
            && let Some(object) = document.as_object_mut()
            && let Some(value) = object.remove(from)
        {
            object.insert(to.to_owned(), value);
        }
    }
    let documents = match document {
        Value::Array(documents) => documents,
        document => vec![document],
    };
    Ok(documents.into_iter().map(numbers_as_floats).collect())
}

fn zoll_inputs(corpus: &Path, name: &str) -> Result<Vec<Value>, Box<dyn Error>> {
    let output = zoll()
        .current_dir(corpus)
        .args(["inspect", name])
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{name}: {stderr}");
    let evaluations: Vec<Value> = serde_json::from_slice(&output.stdout)?;
    // The file's own documents; the fragments in it are not conftest's.
    Ok(evaluations
        .into_iter()
        .filter(|evaluation| evaluation["data"]["zoll"]["file"]["host"].is_null())
        .map(|mut evaluation| numbers_as_floats(evaluation["input"].take()))
        .collect())
}

/// conftest's numbers went through Go's `float64`; 1 and 1.0 are one number
/// to Rego as well.
fn numbers_as_floats(value: Value) -> Value {
    match value {
        Value::Number(n) => n
            .as_f64()
            .and_then(serde_json::Number::from_f64)
            .map_or(Value::Number(n), Value::Number),
        Value::Array(items) => Value::Array(items.into_iter().map(numbers_as_floats).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, numbers_as_floats(v)))
                .collect(),
        ),
        other => other,
    }
}
