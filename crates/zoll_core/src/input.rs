//! What a policy is given for a file and for each fragment in it: `input`,
//! and the facts zoll and conftest put under `data` (docs/design.md,
//! sections 8.1 and 8.3).

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use serde_json::{Value, json};
use tree_sitter::{Node, Tree};

use crate::code;
use crate::document::{self, Document, Origin, Place, Step};
use crate::error::{Error, SourceError};
use crate::injection::{self, Map, Target};
use crate::language;
use crate::policy::Severity;
use crate::walk::File;

/// The rule id of a file or fragment that does not parse (docs/design.md,
/// section 14).
pub const PARSE: &str = "zoll.parse";

/// The rule id of a fragment of a language this zoll has no grammar for
/// (docs/design.md, section 14).
pub const UNCHECKED: &str = "zoll.unchecked";

/// How deep fragments are looked for in fragments.
const MAX_NESTING: usize = 8;

/// One evaluation of the policies: a document and what is known of it.
#[derive(Debug, Serialize)]
pub struct Evaluation {
    /// The document, `input` to the policy.
    pub input: Value,
    /// `data.conftest.file` and `data.zoll.file`.
    pub data: Value,
    /// Where in its text the document came from.
    #[serde(skip)]
    pub origin: Origin,
    /// That text: the file's, or a fragment's.
    #[serde(skip)]
    pub source: Arc<Source>,
}

/// The text a document was read from, and where it is in the file.
#[derive(Debug)]
pub struct Source {
    /// Its language.
    pub language: &'static str,
    /// The text: the file's, or a fragment's, decoded.
    pub text: String,
    /// Its bytes to the file's.
    pub map: Map,
}

/// What zoll itself has to say about the code in a file: a fragment it did
/// not check ([`UNCHECKED`], a notice), or one whose tree it refused
/// ([`PARSE`], a warning, as for a file).
#[derive(Debug)]
pub struct Note {
    /// [`PARSE`] or [`UNCHECKED`].
    pub rule: &'static str,
    /// Its weight.
    pub severity: Severity,
    /// Why.
    pub message: String,
    /// The bytes of the file it is about.
    pub span: Range<usize>,
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

/// The evaluations of a file of this text: one per document, then one per
/// document of each fragment in it, to any depth; and what zoll notes on
/// the way: the code it did not check.
///
/// # Errors
///
/// The file does not parse, or is of a language zoll was built without, or
/// a query of `queries/` does not load.
pub fn evaluations(file: &File, text: &str) -> Result<(Vec<Evaluation>, Vec<Note>), Error> {
    let mut walk = Walk {
        file,
        budget: code::text_budget(text.len()),
        evaluations: Vec::new(),
        notes: Vec::new(),
    };
    let source = Arc::new(Source {
        language: file.language,
        text: text.to_owned(),
        map: Map::identity(text.len()),
    });
    let zoll = json!({"path": file.relative, "language": file.language});
    walk.source(&source, &zoll, 0)?;
    Ok((walk.evaluations, walk.notes))
}

struct Walk<'a> {
    file: &'a File,
    /// The text the file's trees may still hold, its fragments' included.
    budget: std::cell::Cell<usize>,
    evaluations: Vec<Evaluation>,
    notes: Vec<Note>,
}

impl Walk<'_> {
    /// The evaluations of `source`, whose `data.zoll.file` is `zoll`, then
    /// those of its fragments.
    fn source(&mut self, source: &Arc<Source>, zoll: &Value, depth: usize) -> Result<(), Error> {
        let text = source.text.as_str();
        let failed = |message: String, span| {
            Error::Source(SourceError {
                path: self.file.path.clone(),
                text: text.to_owned(),
                message,
                span,
            })
        };
        let documents = document::documents(source.language, &self.file.path, text)?;
        let (documents, tree) = if let Some(documents) = documents {
            // YAML's tree is where its fragments are; JSON and TOML hold none
            // the queries find.
            let tree = code::parse(source.language, text).and_then(Result::ok);
            (documents, tree)
        } else {
            let Some(tree) = code::parse(source.language, text) else {
                return Err(Error::Other(format!(
                    "{}: this zoll was built without {}; `zoll languages` lists what it parses",
                    self.file.relative, source.language
                )));
            };
            let tree = tree.map_err(|message| failed(message, None))?;
            let value = code::value(source.language, text, &tree, &self.budget)
                .map_err(|message| failed(message, None))?;
            let document = Document {
                value,
                origin: Origin::default(),
            };
            (vec![document], Some(tree))
        };
        let data = data(self.file, zoll);
        for document in documents {
            self.evaluations.push(Evaluation {
                input: document.value,
                data: data.clone(),
                origin: document.origin,
                source: Arc::clone(source),
            });
        }
        match tree {
            Some(tree) if depth < MAX_NESTING => self.fragments(source, zoll, &tree, depth),
            _ => Ok(()),
        }
    }

    fn fragments(
        &mut self,
        source: &Arc<Source>,
        zoll: &Value,
        tree: &Tree,
        depth: usize,
    ) -> Result<(), Error> {
        let text = source.text.as_str();
        let fragments = injection::fragments(source.language, text, tree).map_err(Error::Other)?;
        if fragments.is_empty() {
            return Ok(());
        }
        let paths = document::is_data(source.language).then(|| document::yaml_paths(text));
        for fragment in fragments {
            // A note points at the first line of the code, however long.
            let node = fragment.node.clone();
            let line = text[node.clone()]
                .find('\n')
                .map_or(node.end, |i| node.start + i);
            let span = source.map.span(node.start..line);
            let language = match fragment.language {
                Target::Zoll(language) if language::compiled(language) => language,
                Target::Zoll(language) => {
                    self.unchecked(span, &format!("this zoll was built without {language}"));
                    continue;
                }
                Target::Lacking(name) => {
                    self.unchecked(span, &format!("zoll has no grammar for {name}"));
                    continue;
                }
            };
            let at = match &paths {
                Some(paths) => path_at(paths, tree, &fragment.node),
                None => node_at(text, &fragment.kind, fragment.node.clone()),
            };
            let mut host = zoll.clone();
            if let Some(host) = host.as_object_mut() {
                host.insert("at".to_owned(), at);
            }
            let child = Arc::new(Source {
                language,
                map: fragment.map.then(&source.map),
                text: fragment.text,
            });
            let zoll = json!({"path": self.file.relative, "language": language, "host": host});
            match self.source(&child, &zoll, depth + 1) {
                // Data that does not parse as what its host says it is (the
                // body of Rust's `json!`, an example cut short) was not
                // checked: a notice. Code always parses; a tree zoll refuses
                // (too deep, too much text) is a warning, as for a file.
                Err(Error::Source(error)) => {
                    let span = child.map.span(error.span.unwrap_or(0..0));
                    let said = if document::is_data(language) {
                        Note {
                            rule: UNCHECKED,
                            severity: Severity::Notice,
                            message: format!(
                                "this code is not checked: it does not parse as {language}: {}",
                                error.message
                            ),
                            span,
                        }
                    } else {
                        Note {
                            rule: PARSE,
                            severity: Severity::Warn,
                            message: format!("{language} in it: {}", error.message),
                            span,
                        }
                    };
                    self.notes.push(said);
                }
                result => result?,
            }
        }
        Ok(())
    }

    fn unchecked(&mut self, span: Range<usize>, why: &str) {
        self.notes.push(Note {
            rule: UNCHECKED,
            severity: Severity::Notice,
            message: format!("this code is not checked: {why}"),
            span,
        });
    }
}

/// Where a fragment sits in a data document: the path of the nearest of its
/// nodes that has one.
fn path_at(
    paths: &std::collections::HashMap<usize, Vec<Step>>,
    tree: &Tree,
    node: &Range<usize>,
) -> Value {
    let mut at: Option<Node<'_>> = tree
        .root_node()
        .descendant_for_byte_range(node.start, node.end);
    while let Some(node) = at {
        if let Some(path) = paths.get(&node.start_byte()) {
            return Value::Array(path.iter().map(Step::to_json).collect());
        }
        at = node.parent();
    }
    Value::Array(Vec::new())
}

/// Where a fragment sits in code: its node, as the host's tree has it
/// (section 8.2), without what is under it.
fn node_at(text: &str, kind: &str, node: Range<usize>) -> Value {
    let position = |byte: usize| {
        let place = Place::of(text, byte..byte);
        json!({"line": place.line, "column": place.column})
    };
    json!({
        "type": kind,
        "text": text.get(node.clone()),
        "start": position(node.start),
        "end": position(node.end),
    })
}

fn data(file: &File, zoll: &Value) -> Value {
    let name = file.path.file_name().map(|n| n.to_string_lossy());
    let dir = file.path.parent().map(Path::to_string_lossy);
    json!({
        // As conftest sets it: the name, and the absolute directory.
        "conftest": {"file": {"name": name, "dir": dir}},
        "zoll": {"file": zoll},
    })
}
