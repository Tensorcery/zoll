//! The command as a user runs it: the binary, its arguments, its output.

mod code;
mod commands;
mod config;
mod corpus;
mod files;
mod formats;
mod fragments;
mod git;
mod inspect;
mod policies;
mod suppress;

use insta_cmd::get_cargo_bin;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

pub(crate) type TestResult = Result<(), Box<dyn Error>>;

/// A bash policy: a pipe into a shell downloads and runs code nobody
/// reviewed.
pub(crate) const CURL_PIPE: &str = r#"package bash

deny_curl_pipe contains {"msg": "curl piped into a shell", "at": node} if {
	walk(input, [_, node])
	node.type == "pipeline"
	some cmd in node.children
	cmd.type == "command"
	cmd.fields.name.text == "curl"
}
"#;

pub(crate) fn zoll() -> Command {
    Command::new(get_cargo_bin("zoll"))
}

/// A directory with these files in it, standing in for a repository.
pub(crate) struct Repo(TempDir);

impl Repo {
    pub(crate) fn new(files: &[(&str, &str)]) -> Result<Self, Box<dyn Error>> {
        let dir = TempDir::new()?;
        for (path, text) in files {
            let path = dir.path().join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, text)?;
        }
        Ok(Self(dir))
    }

    /// Where it is.
    pub(crate) fn path(&self) -> &Path {
        self.0.path()
    }

    /// `zoll` started in `subdir` of the repository.
    pub(crate) fn zoll(&self, subdir: &str) -> Command {
        let mut command = zoll();
        command.current_dir(self.0.path().join(subdir));
        command
    }

    /// Snapshot settings that write the repository's path as `[REPO]`, in
    /// each spelling zoll may print it: resolved or as given, with `\` or
    /// `/` between its parts, and inside a JSON string.
    pub(crate) fn settings(&self) -> Result<insta::Settings, Box<dyn Error>> {
        let given = self.0.path().display().to_string();
        let resolved = self.0.path().canonicalize()?.display().to_string();
        // Windows resolves `D:\x` as `\\?\D:\x`.
        let resolved = resolved
            .strip_prefix(r"\\?\")
            .unwrap_or(&resolved)
            .to_owned();
        let mut spellings: Vec<String> = Vec::new();
        for path in [resolved, given] {
            for spelling in [path.replace('\\', r"\\"), path.replace('\\', "/"), path] {
                if !spellings.contains(&spelling) {
                    spellings.push(spelling);
                }
            }
        }
        // The longest first: one spelling must not leave a part of another.
        spellings.sort_by_key(|s| std::cmp::Reverse(s.len()));
        let mut settings = insta::Settings::clone_current();
        for path in spellings {
            settings.add_filter(&escape(&path), "[REPO]");
        }
        if cfg!(windows) {
            // A path in JSON past [REPO], as Unix spells it: no snapshot
            // holds a `\\` of its own.
            settings.add_filter(r"\\\\", "/");
            // A file that is not there, as Unix says it.
            settings.add_filter(
                r"The system cannot find the file specified\.",
                "No such file or directory",
            );
        }
        Ok(settings)
    }
}

/// A text as a regex that matches it alone.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}
