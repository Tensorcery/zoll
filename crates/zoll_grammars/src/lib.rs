//! The tree-sitter grammars zoll vendors, each behind a feature of its name:
//! those with no crate on crates.io under a permissive license. Where each
//! comes from is in `vendor/README.md`.

#[cfg(any(feature = "awk", feature = "jq"))]
use tree_sitter_language::LanguageFn;

#[cfg(feature = "awk")]
#[allow(unsafe_code)]
unsafe extern "C" {
    fn tree_sitter_awk() -> *const ();
}

/// awk's grammar.
#[cfg(feature = "awk")]
#[allow(unsafe_code)]
pub const AWK: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_awk) };

/// awk's node types, as `node-types.json` has them.
#[cfg(feature = "awk")]
pub const AWK_NODE_TYPES: &str = include_str!("../vendor/awk/src/node-types.json");

#[cfg(feature = "jq")]
#[allow(unsafe_code)]
unsafe extern "C" {
    fn tree_sitter_jq() -> *const ();
}

/// jq's grammar.
#[cfg(feature = "jq")]
#[allow(unsafe_code)]
pub const JQ: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_jq) };

/// jq's node types, as `node-types.json` has them.
#[cfg(feature = "jq")]
pub const JQ_NODE_TYPES: &str = include_str!("../vendor/jq/src/node-types.json");
