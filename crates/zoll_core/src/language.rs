//! The language of a file: the config's `languages` table, then the file's
//! name and extension, then its shebang (docs/design.md, section 5).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use globset::{Glob, GlobBuilder, GlobMatcher};

use crate::error::Error;

/// The languages of the base set, by their tree-sitter-language-pack names
/// (docs/design.md, section 7).
pub const LANGUAGES: [&str; 30] = [
    "awk",
    "bash",
    "c",
    "c_sharp",
    "cpp",
    "css",
    "dockerfile",
    "go",
    "groovy",
    "hcl",
    "html",
    "java",
    "javascript",
    "jq",
    "json",
    "kotlin",
    "make",
    "markdown",
    "php",
    "powershell",
    "python",
    "ruby",
    "rust",
    "sql",
    "swift",
    "toml",
    "tsx",
    "typescript",
    "xml",
    "yaml",
];

/// File names that tell their language whole.
const NAMES: [(&str, &str); 11] = [
    ("Containerfile", "dockerfile"),
    ("Dockerfile", "dockerfile"),
    ("GNUmakefile", "make"),
    ("Gemfile", "ruby"),
    ("Jenkinsfile", "groovy"),
    ("Makefile", "make"),
    ("Rakefile", "ruby"),
    ("makefile", "make"),
    ("Pipfile", "toml"),
    ("go.mod", "go"),
    (".bashrc", "bash"),
];

/// Extensions, without the dot.
const EXTENSIONS: [(&str, &str); 52] = [
    ("awk", "awk"),
    ("bash", "bash"),
    ("c", "c"),
    ("cc", "cpp"),
    ("cjs", "javascript"),
    ("cpp", "cpp"),
    ("cs", "c_sharp"),
    ("css", "css"),
    ("cts", "typescript"),
    ("cxx", "cpp"),
    ("dockerfile", "dockerfile"),
    ("go", "go"),
    ("gradle", "groovy"),
    ("groovy", "groovy"),
    ("h", "c"),
    ("hcl", "hcl"),
    ("hh", "cpp"),
    ("hpp", "cpp"),
    ("htm", "html"),
    ("html", "html"),
    ("hxx", "cpp"),
    ("java", "java"),
    ("jq", "jq"),
    ("js", "javascript"),
    ("json", "json"),
    ("jsx", "javascript"),
    ("kt", "kotlin"),
    ("kts", "kotlin"),
    ("markdown", "markdown"),
    ("md", "markdown"),
    ("mjs", "javascript"),
    ("mk", "make"),
    ("mts", "typescript"),
    ("php", "php"),
    ("ps1", "powershell"),
    ("psd1", "powershell"),
    ("psm1", "powershell"),
    ("py", "python"),
    ("pyi", "python"),
    ("rb", "ruby"),
    ("rs", "rust"),
    ("sh", "bash"),
    ("sql", "sql"),
    ("swift", "swift"),
    ("tf", "hcl"),
    ("tfvars", "hcl"),
    ("toml", "toml"),
    ("ts", "typescript"),
    ("tsx", "tsx"),
    ("xml", "xml"),
    ("yaml", "yaml"),
    ("yml", "yaml"),
];

/// Interpreters a shebang may name.
const INTERPRETERS: [(&str, &str); 17] = [
    ("awk", "awk"),
    ("bash", "bash"),
    ("bun", "javascript"),
    ("dash", "bash"),
    ("deno", "typescript"),
    ("gawk", "awk"),
    ("groovy", "groovy"),
    ("jq", "jq"),
    ("ksh", "bash"),
    ("make", "make"),
    ("mawk", "awk"),
    ("node", "javascript"),
    ("php", "php"),
    ("pwsh", "powershell"),
    ("python", "python"),
    ("ruby", "ruby"),
    ("sh", "bash"),
];

/// How the languages of files are told, with the config's table compiled.
#[derive(Debug)]
pub struct Languages {
    /// The config's globs, in the table's order: a glob with a `/` matches
    /// the path from the config's directory, one without matches the file
    /// name, wherever the file is.
    table: Vec<(GlobMatcher, bool, &'static str)>,
}

impl Languages {
    /// The config's `languages` table, checked: every glob compiles, every
    /// language is one zoll knows.
    ///
    /// # Errors
    ///
    /// A glob that does not compile, or a language not in [`LANGUAGES`].
    pub fn new(table: &BTreeMap<String, String>) -> Result<Self, Error> {
        let table = table
            .iter()
            .map(|(glob, language)| {
                let Some(language) = LANGUAGES.iter().find(|known| *known == language) else {
                    return Err(Error::Other(format!(
                        "the config binds `{glob}` to `{language}`, which is no language \
                         zoll knows; `zoll languages` lists them"
                    )));
                };
                Ok((
                    glob_of(glob)?.compile_matcher(),
                    glob.contains('/'),
                    *language,
                ))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { table })
    }

    /// The language of a file. `relative` is its path from the config's
    /// directory; `path` is where to read its shebang.
    #[must_use]
    pub fn of(&self, relative: &Path, path: &Path) -> Option<&'static str> {
        self.in_config(relative)
            .or_else(|| from_name(relative))
            .or_else(|| from_shebang(path))
    }

    fn in_config(&self, relative: &Path) -> Option<&'static str> {
        let name = Path::new(relative.file_name()?);
        self.table
            .iter()
            .find(|(glob, whole_path, _)| glob.is_match(if *whole_path { relative } else { name }))
            .map(|(_, _, language)| *language)
    }
}

/// A glob as zoll reads one: `*` stays inside a directory, `**` crosses them,
/// as in `.gitignore`.
pub(crate) fn glob_of(glob: &str) -> Result<Glob, Error> {
    GlobBuilder::new(glob)
        .literal_separator(true)
        .build()
        .map_err(|e| Error::Other(format!("`{glob}`: {e}")))
}

fn from_name(path: &Path) -> Option<&'static str> {
    let name = path.file_name()?.to_str()?;
    if let Some((_, language)) = NAMES.iter().find(|(known, _)| *known == name) {
        return Some(language);
    }
    // `Dockerfile.dev`, `Makefile.release`: the name before the dot.
    if let Some((stem, _)) = name.split_once('.')
        && let Some((_, language)) = NAMES.iter().find(|(known, _)| *known == stem)
    {
        return Some(language);
    }
    let extension = path.extension()?.to_str()?;
    EXTENSIONS
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, language)| *language)
}

/// The language a `#!` line names: `#!/bin/sh`, `#!/usr/bin/env python3`,
/// `#!/usr/bin/env -S awk -f`.
fn from_shebang(path: &Path) -> Option<&'static str> {
    let mut head = [0; 128];
    let read = File::open(path).ok()?.read(&mut head).ok()?;
    let line = std::str::from_utf8(&head[..read]).ok()?.lines().next()?;
    let mut words = line.strip_prefix("#!")?.split_whitespace();
    let mut program = words.next()?.rsplit('/').next()?;
    if program == "env" {
        // `env` takes flags (`-S`) and `NAME=value` before the program.
        program = words.find(|word| !word.starts_with('-') && !word.contains('='))?;
    }
    // `python3.12` is python.
    let program = program.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    INTERPRETERS
        .iter()
        .find(|(known, _)| *known == program)
        .map(|(_, language)| *language)
}
