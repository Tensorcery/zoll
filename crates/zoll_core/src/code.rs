//! Code into the tree a policy is given (docs/design.md, section 8.2).

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, OnceLock};

use serde::Deserialize;
use serde_json::{Map, Value, json};
use tree_sitter::{Language, Node, Parser, Point, Tree};

use crate::language::LANGUAGES;

/// How deep a tree may be: deeper ones are refused, not walked into a
/// stack overflow.
const MAX_DEPTH: usize = 512;

/// How many times the file's size the text of all nodes may take. Every
/// node repeats the text under it, so a deep tree copies the file many times
/// over; real code takes 6 to 11 (docs/design.md, section 8.2).
const MAX_TEXT_RATIO: usize = 64;

/// The text every tree may hold, however small its file.
const MIN_TEXT: usize = 1 << 20;

/// The nodes the trees of one file may hold in all. A node takes about 3 KB
/// as a policy's input, so a dense file of `a;` would ask for gigabytes; a
/// minified jQuery is 37 000 nodes.
// ponytail: a fixed cap; a leaner node would let it grow.
const MAX_NODES: usize = 1_000_000;

/// What the trees of one file may still hold in all: its own and its
/// fragments', to any depth.
#[derive(Debug)]
pub struct Budget {
    /// Bytes of node text.
    text: Cell<usize>,
    nodes: Cell<usize>,
}

impl Budget {
    /// The budget of a file of `len` bytes.
    #[must_use]
    pub fn new(len: usize) -> Self {
        Self {
            text: Cell::new(len.saturating_mul(MAX_TEXT_RATIO).max(MIN_TEXT)),
            nodes: Cell::new(MAX_NODES),
        }
    }

    /// Spends one node of `text` bytes, or says what there is too much of.
    fn spend(&self, text: usize) -> Result<(), String> {
        let Some(nodes) = self.nodes.get().checked_sub(1) else {
            return Err("its trees hold more than a million nodes".to_owned());
        };
        let Some(left) = self.text.get().checked_sub(text) else {
            return Err(format!(
                "the text of its nodes is more than {MAX_TEXT_RATIO} times the file's"
            ));
        };
        self.nodes.set(nodes);
        self.text.set(left);
        Ok(())
    }
}

/// The grammar of a language and its `node-types.json`, when zoll was built
/// with it. YAML's is always there: its documents are read as data, its tree
/// is where the injection queries look.
// A table: one arm per language, as long as the build has languages.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn grammar(language: &str) -> Option<(Language, &'static str)> {
    match language {
        "yaml" => Some((
            tree_sitter_yaml::LANGUAGE.into(),
            tree_sitter_yaml::NODE_TYPES,
        )),
        #[cfg(feature = "awk")]
        "awk" => Some((zoll_grammars::AWK.into(), zoll_grammars::AWK_NODE_TYPES)),
        #[cfg(feature = "bash")]
        "bash" => Some((
            tree_sitter_bash::LANGUAGE.into(),
            tree_sitter_bash::NODE_TYPES,
        )),
        #[cfg(feature = "c")]
        "c" => Some((tree_sitter_c::LANGUAGE.into(), tree_sitter_c::NODE_TYPES)),
        #[cfg(feature = "c_sharp")]
        "c_sharp" => Some((
            tree_sitter_c_sharp::LANGUAGE.into(),
            tree_sitter_c_sharp::NODE_TYPES,
        )),
        #[cfg(feature = "cmake")]
        "cmake" => Some((
            tree_sitter_cmake::LANGUAGE.into(),
            tree_sitter_cmake::NODE_TYPES,
        )),
        #[cfg(feature = "cpp")]
        "cpp" => Some((
            tree_sitter_cpp::LANGUAGE.into(),
            tree_sitter_cpp::NODE_TYPES,
        )),
        #[cfg(feature = "css")]
        "css" => Some((
            tree_sitter_css::LANGUAGE.into(),
            tree_sitter_css::NODE_TYPES,
        )),
        #[cfg(feature = "dart")]
        "dart" => Some((
            tree_sitter_dart::LANGUAGE.into(),
            tree_sitter_dart::NODE_TYPES,
        )),
        #[cfg(feature = "dockerfile")]
        "dockerfile" => Some((
            tree_sitter_containerfile::LANGUAGE.into(),
            tree_sitter_containerfile::NODE_TYPES,
        )),
        #[cfg(feature = "elixir")]
        "elixir" => Some((
            tree_sitter_elixir::LANGUAGE.into(),
            tree_sitter_elixir::NODE_TYPES,
        )),
        #[cfg(feature = "go")]
        "go" => Some((tree_sitter_go::LANGUAGE.into(), tree_sitter_go::NODE_TYPES)),
        #[cfg(feature = "gotmpl")]
        "gotmpl" => Some((
            zoll_grammars::GOTMPL.into(),
            zoll_grammars::GOTMPL_NODE_TYPES,
        )),
        #[cfg(feature = "graphql")]
        "graphql" => Some((
            tree_sitter_graphql::LANGUAGE.into(),
            tree_sitter_graphql::NODE_TYPES,
        )),
        #[cfg(feature = "groovy")]
        "groovy" => Some((
            tree_sitter_groovy::LANGUAGE.into(),
            tree_sitter_groovy::NODE_TYPES,
        )),
        #[cfg(feature = "haskell")]
        "haskell" => Some((
            tree_sitter_haskell::LANGUAGE.into(),
            tree_sitter_haskell::NODE_TYPES,
        )),
        #[cfg(feature = "hcl")]
        "hcl" => Some((
            tree_sitter_hcl::LANGUAGE.into(),
            tree_sitter_hcl::NODE_TYPES,
        )),
        #[cfg(feature = "html")]
        "html" => Some((
            tree_sitter_html::LANGUAGE.into(),
            tree_sitter_html::NODE_TYPES,
        )),
        #[cfg(feature = "ini")]
        "ini" => Some((
            tree_sitter_ini::LANGUAGE.into(),
            tree_sitter_ini::NODE_TYPES,
        )),
        #[cfg(feature = "java")]
        "java" => Some((
            tree_sitter_java::LANGUAGE.into(),
            tree_sitter_java::NODE_TYPES,
        )),
        #[cfg(feature = "javascript")]
        "javascript" => Some((
            tree_sitter_javascript::LANGUAGE.into(),
            tree_sitter_javascript::NODE_TYPES,
        )),
        #[cfg(feature = "jq")]
        "jq" => Some((zoll_grammars::JQ.into(), zoll_grammars::JQ_NODE_TYPES)),
        #[cfg(feature = "jsonnet")]
        "jsonnet" => Some((
            tree_sitter_jsonnet::LANGUAGE.into(),
            tree_sitter_jsonnet::NODE_TYPES,
        )),
        #[cfg(feature = "just")]
        "just" => Some((zoll_grammars::JUST.into(), zoll_grammars::JUST_NODE_TYPES)),
        #[cfg(feature = "kotlin")]
        "kotlin" => Some((
            tree_sitter_kotlin_ng::LANGUAGE.into(),
            tree_sitter_kotlin_ng::NODE_TYPES,
        )),
        #[cfg(feature = "lua")]
        "lua" => Some((
            tree_sitter_lua::LANGUAGE.into(),
            tree_sitter_lua::NODE_TYPES,
        )),
        #[cfg(feature = "make")]
        "make" => Some((
            tree_sitter_make::LANGUAGE.into(),
            tree_sitter_make::NODE_TYPES,
        )),
        #[cfg(feature = "markdown")]
        "markdown" => Some((
            tree_sitter_md::LANGUAGE.into(),
            tree_sitter_md::NODE_TYPES_BLOCK,
        )),
        #[cfg(feature = "nginx")]
        "nginx" => Some((
            tree_sitter_nginx::LANGUAGE.into(),
            tree_sitter_nginx::NODE_TYPES,
        )),
        #[cfg(feature = "nix")]
        "nix" => Some((
            tree_sitter_nix::LANGUAGE.into(),
            tree_sitter_nix::NODE_TYPES,
        )),
        #[cfg(feature = "perl")]
        "perl" => Some((zoll_grammars::PERL.into(), zoll_grammars::PERL_NODE_TYPES)),
        #[cfg(feature = "php")]
        "php" => Some((
            tree_sitter_php::LANGUAGE_PHP.into(),
            tree_sitter_php::PHP_NODE_TYPES,
        )),
        #[cfg(feature = "powershell")]
        "powershell" => Some((
            tree_sitter_powershell::LANGUAGE.into(),
            tree_sitter_powershell::NODE_TYPES,
        )),
        #[cfg(feature = "proto")]
        "proto" => Some((
            tree_sitter_proto::LANGUAGE.into(),
            tree_sitter_proto::NODE_TYPES,
        )),
        #[cfg(feature = "python")]
        "python" => Some((
            tree_sitter_python::LANGUAGE.into(),
            tree_sitter_python::NODE_TYPES,
        )),
        #[cfg(feature = "regex")]
        "regex" => Some((
            tree_sitter_regex::LANGUAGE.into(),
            tree_sitter_regex::NODE_TYPES,
        )),
        #[cfg(feature = "ruby")]
        "ruby" => Some((
            tree_sitter_ruby::LANGUAGE.into(),
            tree_sitter_ruby::NODE_TYPES,
        )),
        #[cfg(feature = "rust")]
        "rust" => Some((
            tree_sitter_rust::LANGUAGE.into(),
            tree_sitter_rust::NODE_TYPES,
        )),
        #[cfg(feature = "scala")]
        "scala" => Some((
            tree_sitter_scala::LANGUAGE.into(),
            tree_sitter_scala::NODE_TYPES,
        )),
        #[cfg(feature = "scss")]
        "scss" => Some((zoll_grammars::SCSS.into(), zoll_grammars::SCSS_NODE_TYPES)),
        #[cfg(feature = "sql")]
        "sql" => Some((
            tree_sitter_sequel::LANGUAGE.into(),
            tree_sitter_sequel::NODE_TYPES,
        )),
        #[cfg(feature = "starlark")]
        "starlark" => Some((
            tree_sitter_starlark::LANGUAGE.into(),
            tree_sitter_starlark::NODE_TYPES,
        )),
        #[cfg(feature = "svelte")]
        "svelte" => Some((
            tree_sitter_svelte_ng::LANGUAGE.into(),
            tree_sitter_svelte_ng::NODE_TYPES,
        )),
        #[cfg(feature = "swift")]
        "swift" => Some((
            tree_sitter_swift::LANGUAGE.into(),
            tree_sitter_swift::NODE_TYPES,
        )),
        #[cfg(feature = "tsx")]
        "tsx" => Some((
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            tree_sitter_typescript::TSX_NODE_TYPES,
        )),
        #[cfg(feature = "typescript")]
        "typescript" => Some((
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            tree_sitter_typescript::TYPESCRIPT_NODE_TYPES,
        )),
        #[cfg(feature = "xml")]
        "xml" => Some((
            tree_sitter_xml::LANGUAGE_XML.into(),
            tree_sitter_xml::XML_NODE_TYPES,
        )),
        #[cfg(feature = "zig")]
        "zig" => Some((
            tree_sitter_zig::LANGUAGE.into(),
            tree_sitter_zig::NODE_TYPES,
        )),
        _ => None,
    }
}

/// Whether zoll was built with the grammar of `language`.
#[must_use]
pub fn compiled(language: &str) -> bool {
    grammar(language).is_some()
}

/// The tree-sitter tree of `text`, a file of `language`; `None` when zoll
/// was built without its grammar.
///
/// # Errors
///
/// The grammar does not load.
#[must_use]
pub fn parse(language: &str, text: &str) -> Option<Result<Tree, String>> {
    let (grammar, _) = grammar(language)?;
    let mut parser = Parser::new();
    if let Err(e) = parser.set_language(&grammar) {
        return Some(Err(format!("the grammar of {language} does not load: {e}")));
    }
    Some(
        parser
            .parse(text, None)
            .ok_or_else(|| "the parser stopped".to_owned()),
    )
}

/// The tree a policy is given for `tree`, the tree-sitter tree of `text`, a
/// file of `language`.
///
/// # Errors
///
/// The tree is deeper than zoll walks, or holds more text or nodes than
/// `budget` has left, which it spends.
pub fn value(language: &str, text: &str, tree: &Tree, budget: &Budget) -> Result<Value, String> {
    let node_types = grammar(language).map_or("[]", |(_, node_types)| node_types);
    let builder = Builder {
        text,
        multiple: multiple(language, node_types),
        columns: Columns::new(text),
        budget,
    };
    builder.node(tree.root_node(), 0)
}

/// The fields that may hold several nodes, by the type of their node.
type Multiple = HashMap<String, HashSet<String>>;

/// Each language's, read from its `node-types.json` once, when first needed.
static MULTIPLE: LazyLock<HashMap<&'static str, OnceLock<Multiple>>> =
    LazyLock::new(|| LANGUAGES.iter().map(|&l| (l, OnceLock::new())).collect());

static NONE: LazyLock<Multiple> = LazyLock::new(Multiple::new);

/// A node type of `node-types.json`, as far as zoll reads it.
#[derive(Deserialize)]
struct NodeType {
    r#type: String,
    #[serde(default)]
    fields: HashMap<String, Field>,
}

#[derive(Deserialize)]
struct Field {
    multiple: bool,
}

fn multiple(language: &str, node_types: &str) -> &'static Multiple {
    let Some(cell) = MULTIPLE.get(language) else {
        return &NONE;
    };
    cell.get_or_init(|| {
        // The file comes with the grammar: one that does not read leaves
        // every field a single node.
        serde_json::from_str::<Vec<NodeType>>(node_types)
            .unwrap_or_default()
            .into_iter()
            .map(|node| {
                let fields = node
                    .fields
                    .into_iter()
                    .filter(|(_, field)| field.multiple)
                    .map(|(name, _)| name)
                    .collect();
                (node.r#type, fields)
            })
            // A keyword shares its name with the node it starts (Haskell's
            // `do`), and has no fields to take the node's place with.
            .filter(|(_, fields): &(_, HashSet<_>)| !fields.is_empty())
            .collect()
    })
}

struct Builder<'a> {
    text: &'a str,
    multiple: &'a Multiple,
    columns: Columns,
    /// What the nodes not built yet may still take.
    budget: &'a Budget,
}

impl Builder<'_> {
    fn node(&self, node: Node<'_>, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err(format!("the tree is nested deeper than {MAX_DEPTH}"));
        }
        let mut fields = Map::new();
        let mut children = Vec::new();
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                // A missing node is the parser's guess at what the text
                // lacks: nothing in the file to point at.
                if child.is_named() && !child.is_missing() {
                    let value = self.node(child, depth + 1)?;
                    match cursor.field_name() {
                        Some(field) => self.field(&mut fields, node.kind(), field, value),
                        None => children.push(value),
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        let mut object = Map::new();
        object.insert("type".to_owned(), node.kind().into());
        object.insert(
            "start".to_owned(),
            self.position(node.start_position(), node.start_byte()),
        );
        object.insert(
            "end".to_owned(),
            self.position(node.end_position(), node.end_byte()),
        );
        // Every node, not the leaves only: a policy reads a node's text
        // without knowing how its grammar nests it.
        let text = self.text.get(node.byte_range()).unwrap_or_default();
        self.budget.spend(text.len())?;
        object.insert("text".to_owned(), text.into());
        if !fields.is_empty() {
            object.insert("fields".to_owned(), Value::Object(fields));
        }
        if !children.is_empty() {
            object.insert("children".to_owned(), Value::Array(children));
        }
        Ok(Value::Object(object))
    }

    /// A field that may repeat is a list even with one node, so a policy
    /// reads it the same way in every file.
    fn field(&self, fields: &mut Map<String, Value>, kind: &str, field: &str, value: Value) {
        let several = self.multiple.get(kind).is_some_and(|f| f.contains(field));
        match fields.get_mut(field) {
            Some(Value::Array(nodes)) => nodes.push(value),
            // A field the grammar has once, met twice: in the parts of a
            // tree that did not parse, where anything goes.
            Some(first) => *first = Value::Array(vec![first.take(), value]),
            None => {
                let value = if several {
                    Value::Array(vec![value])
                } else {
                    value
                };
                fields.insert(field.to_owned(), value);
            }
        }
    }

    fn position(&self, point: Point, byte: usize) -> Value {
        let line_start = byte - point.column;
        let column = self.columns.chars(byte) - self.columns.chars(line_start) + 1;
        json!({"line": point.row + 1, "column": column})
    }
}

/// Columns in characters, not bytes, in time that does not grow with the
/// line: per character of more than one byte, the byte past it and how many
/// bytes more than characters the text holds up to there.
struct Columns(Vec<(usize, usize)>);

impl Columns {
    fn new(text: &str) -> Self {
        let mut extra = 0;
        Self(
            text.char_indices()
                .filter(|(_, c)| c.len_utf8() > 1)
                .map(|(i, c)| {
                    extra += c.len_utf8() - 1;
                    (i + c.len_utf8(), extra)
                })
                .collect(),
        )
    }

    /// How many characters the text holds before `byte`.
    fn chars(&self, byte: usize) -> usize {
        let past = self.0.partition_point(|&(end, _)| end <= byte);
        byte - past.checked_sub(1).map_or(0, |i| self.0[i].1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_budget_runs_out_of_nodes() {
        let budget = Budget {
            text: Cell::new(10),
            nodes: Cell::new(1),
        };
        assert!(budget.spend(4).is_ok());
        let error = budget.spend(0).err().unwrap_or_default();
        assert!(error.contains("a million nodes"), "{error}");
        // Nothing spent on a node refused.
        assert_eq!(budget.text.get(), 6);
    }

    // A keyword of the same name, with no fields, does not hide the node's.
    #[cfg(feature = "haskell")]
    #[test]
    fn a_keyword_does_not_hide_its_node_s_fields() {
        let (_, node_types) = grammar("haskell").unwrap_or_else(|| unreachable!());
        let multiple = multiple("haskell", node_types);
        assert!(multiple.get("do").is_some_and(|f| f.contains("statement")));
    }

    // The base set: a build with fewer languages leaves the others out.
    #[cfg(feature = "base")]
    #[test]
    fn every_grammar_loads() {
        for language in LANGUAGES {
            if matches!(language, "yaml" | "json" | "toml") {
                continue;
            }
            let (grammar, node_types) = grammar(language).unwrap_or_else(|| {
                unreachable!("{language} is in the base set but not compiled in")
            });
            assert!(
                Parser::new().set_language(&grammar).is_ok(),
                "{language}: the runtime refuses its grammar"
            );
            assert!(
                serde_json::from_str::<Vec<NodeType>>(node_types).is_ok(),
                "{language}: its node-types.json does not read"
            );
        }
    }
}
