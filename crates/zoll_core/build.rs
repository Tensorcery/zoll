//! Embeds the injection queries of `queries/` (docs/design.md, section 6):
//! one `include_str!` per host language, and the table of language names.

use std::fmt::Write as _;
use std::path::Path;

/// The query files of a language, in the order they are read.
const FILES: [&str; 2] = ["injections.scm", "zoll.scm"];

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../queries");
    println!("cargo::rerun-if-changed={}", root.display());
    // `injections.scm` is copied from Helix; `zoll.scm` is zoll's own, for
    // what Helix's do not find with the grammar zoll has.
    let mut dirs: Vec<_> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|dir| FILES.iter().any(|file| dir.join(file).is_file()))
        .collect();
    dirs.sort();
    let mut out = String::from(
        "/// Each host language's injection queries.\n\
         pub(crate) static QUERIES: &[(&str, &str)] = &[\n",
    );
    for dir in &dirs {
        let name = dir
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let files: Vec<String> = FILES
            .iter()
            .map(|file| dir.join(file))
            .filter(|file| file.is_file())
            .map(|file| {
                println!("cargo::rerun-if-changed={}", file.display());
                format!("include_str!({:?})", file.display().to_string())
            })
            .collect();
        let _ = writeln!(
            out,
            "    ({name:?}, concat!({})),",
            files.join(", \"\\n\", ")
        );
    }
    out.push_str("];\n");
    let table = root.join("languages.toml");
    println!("cargo::rerun-if-changed={}", table.display());
    let table = if table.is_file() {
        format!("include_str!({:?})", table.display().to_string())
    } else {
        "\"\"".to_owned()
    };
    let _ = writeln!(
        out,
        "/// `queries/languages.toml`.\npub(crate) static LANGUAGE_NAMES: &str = {table};"
    );
    let out_dir = std::env::var("OUT_DIR").unwrap_or_default();
    if let Err(e) = std::fs::write(Path::new(&out_dir).join("queries.rs"), out) {
        println!("cargo::error=queries.rs: {e}");
    }
}
