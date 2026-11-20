//! The command as a user runs it: the binary, its arguments, its output.

mod code;
mod commands;
mod config;
mod corpus;
mod files;
mod fragments;
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

    /// `zoll` started in `subdir` of the repository.
    pub(crate) fn zoll(&self, subdir: &str) -> Command {
        let mut command = zoll();
        command.current_dir(self.0.path().join(subdir));
        command
    }

    /// Snapshot settings that write the repository's path as `[REPO]`.
    pub(crate) fn settings(&self) -> Result<insta::Settings, Box<dyn Error>> {
        let path = self.0.path().canonicalize()?;
        let mut settings = insta::Settings::clone_current();
        settings.add_filter(&escape(&path), "[REPO]");
        Ok(settings)
    }
}

fn escape(path: &Path) -> String {
    path.display().to_string().replace('.', r"\.")
}
