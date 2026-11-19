//! Compiles the vendored grammars the enabled features name.

fn main() {
    for (feature, scanner) in [("awk", true), ("jq", false)] {
        if std::env::var_os(format!("CARGO_FEATURE_{}", feature.to_uppercase())).is_none() {
            continue;
        }
        let src = std::path::Path::new("vendor").join(feature).join("src");
        let mut build = cc::Build::new();
        build
            .std("c11")
            .include(&src)
            .file(src.join("parser.c"))
            // Generated code: its warnings are not ours to fix.
            .warnings(false);
        if scanner {
            build.file(src.join("scanner.c"));
        }
        build.compile(&format!("tree-sitter-{feature}"));
        println!("cargo:rerun-if-changed={}", src.display());
    }
}
