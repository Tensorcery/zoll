//! The language of a file: the config's `languages` table, then the file's
//! name and extension, then its shebang (docs/design.md, section 5).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use globset::{Glob, GlobBuilder, GlobMatcher};

use crate::error::Error;

/// The languages of the base set, by zoll's names (docs/design.md,
/// section 7).
pub const LANGUAGES: [&str; 50] = [
    "awk",
    "bash",
    "c",
    "c_sharp",
    "cmake",
    "cpp",
    "css",
    "dart",
    "dockerfile",
    "elixir",
    "go",
    "gotmpl",
    "graphql",
    "groovy",
    "haskell",
    "hcl",
    "html",
    "ini",
    "java",
    "javascript",
    "jq",
    "json",
    "jsonnet",
    "just",
    "kotlin",
    "lua",
    "make",
    "markdown",
    "nginx",
    "nix",
    "perl",
    "php",
    "powershell",
    "proto",
    "python",
    "regex",
    "ruby",
    "rust",
    "scala",
    "scss",
    "sql",
    "starlark",
    "svelte",
    "swift",
    "toml",
    "tsx",
    "typescript",
    "xml",
    "yaml",
    "zig",
];

/// Whether this zoll parses `language`: a data format always, code when it
/// was built with the grammar.
#[must_use]
pub fn compiled(language: &str) -> bool {
    matches!(language, "yaml" | "json" | "toml") || crate::code::compiled(language)
}

/// File names that tell their language whole.
const NAMES: [(&str, &str); 20] = [
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
    ("BUILD", "starlark"),
    ("CMakeLists.txt", "cmake"),
    ("Justfile", "just"),
    ("Tiltfile", "starlark"),
    ("WORKSPACE", "starlark"),
    (".editorconfig", "ini"),
    (".justfile", "just"),
    ("justfile", "just"),
    ("nginx.conf", "nginx"),
];

/// Extensions, without the dot.
const EXTENSIONS: [(&str, &str); 81] = [
    ("awk", "awk"),
    ("bash", "bash"),
    ("bazel", "starlark"),
    ("bzl", "starlark"),
    ("c", "c"),
    ("cc", "cpp"),
    ("cfg", "ini"),
    ("cjs", "javascript"),
    ("cmake", "cmake"),
    ("cpp", "cpp"),
    ("cs", "c_sharp"),
    ("css", "css"),
    ("cts", "typescript"),
    ("cxx", "cpp"),
    ("dart", "dart"),
    ("dockerfile", "dockerfile"),
    ("ex", "elixir"),
    ("exs", "elixir"),
    ("go", "go"),
    ("gotmpl", "gotmpl"),
    ("gql", "graphql"),
    ("gradle", "groovy"),
    ("graphql", "graphql"),
    ("groovy", "groovy"),
    ("h", "c"),
    ("hcl", "hcl"),
    ("hh", "cpp"),
    ("hpp", "cpp"),
    ("hs", "haskell"),
    ("htm", "html"),
    ("html", "html"),
    ("hxx", "cpp"),
    ("ini", "ini"),
    ("java", "java"),
    ("jq", "jq"),
    ("js", "javascript"),
    ("json", "json"),
    ("jsonnet", "jsonnet"),
    ("jsx", "javascript"),
    ("just", "just"),
    ("kt", "kotlin"),
    ("kts", "kotlin"),
    ("libsonnet", "jsonnet"),
    ("lua", "lua"),
    ("markdown", "markdown"),
    ("md", "markdown"),
    ("mjs", "javascript"),
    ("mk", "make"),
    ("mts", "typescript"),
    ("nix", "nix"),
    ("php", "php"),
    ("pl", "perl"),
    ("pm", "perl"),
    ("proto", "proto"),
    ("ps1", "powershell"),
    ("psd1", "powershell"),
    ("psm1", "powershell"),
    ("py", "python"),
    ("pyi", "python"),
    ("rb", "ruby"),
    ("rs", "rust"),
    ("sbt", "scala"),
    ("sc", "scala"),
    ("scala", "scala"),
    ("scss", "scss"),
    ("sh", "bash"),
    ("sql", "sql"),
    ("star", "starlark"),
    ("svelte", "svelte"),
    ("swift", "swift"),
    ("tf", "hcl"),
    ("tfvars", "hcl"),
    ("tmpl", "gotmpl"),
    ("toml", "toml"),
    ("tpl", "gotmpl"),
    ("ts", "typescript"),
    ("tsx", "tsx"),
    ("xml", "xml"),
    ("yaml", "yaml"),
    ("yml", "yaml"),
    ("zig", "zig"),
];

/// Interpreters a shebang may name.
const INTERPRETERS: [(&str, &str); 21] = [
    ("awk", "awk"),
    ("bash", "bash"),
    ("bun", "javascript"),
    ("dash", "bash"),
    ("deno", "typescript"),
    ("elixir", "elixir"),
    ("gawk", "awk"),
    ("groovy", "groovy"),
    ("jq", "jq"),
    ("just", "just"),
    ("ksh", "bash"),
    ("lua", "lua"),
    ("make", "make"),
    ("mawk", "awk"),
    ("node", "javascript"),
    ("perl", "perl"),
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

pub(crate) fn from_name(path: &Path) -> Option<&'static str> {
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
    of_shebang(std::str::from_utf8(&head[..read]).ok()?.lines().next()?)
}

/// The language of a `#!` line.
pub(crate) fn of_shebang(line: &str) -> Option<&'static str> {
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
