//! The tree-sitter grammars zoll vendors, each behind a feature of its name:
//! those with no crate on crates.io under a permissive license that builds
//! with zoll's tree-sitter. Where each comes from is in `vendor/README.md`.

/// A vendored grammar: its `LanguageFn` and its `node-types.json`.
macro_rules! grammar {
    ($feature:literal, $symbol:ident, $language:ident, $node_types:ident) => {
        #[cfg(feature = $feature)]
        #[allow(unsafe_code)]
        unsafe extern "C" {
            fn $symbol() -> *const ();
        }

        #[doc = concat!("The grammar of ", $feature, ".")]
        #[cfg(feature = $feature)]
        #[allow(unsafe_code)]
        pub const $language: tree_sitter_language::LanguageFn =
            unsafe { tree_sitter_language::LanguageFn::from_raw($symbol) };

        #[doc = concat!("The node types of ", $feature, ", as `node-types.json` has them.")]
        #[cfg(feature = $feature)]
        pub const $node_types: &str =
            include_str!(concat!("../vendor/", $feature, "/src/node-types.json"));
    };
}

grammar!("awk", tree_sitter_awk, AWK, AWK_NODE_TYPES);
grammar!("gotmpl", tree_sitter_gotmpl, GOTMPL, GOTMPL_NODE_TYPES);
grammar!("jq", tree_sitter_jq, JQ, JQ_NODE_TYPES);
grammar!("just", tree_sitter_just, JUST, JUST_NODE_TYPES);
grammar!("perl", tree_sitter_perl, PERL, PERL_NODE_TYPES);
grammar!("scss", tree_sitter_scss, SCSS, SCSS_NODE_TYPES);
