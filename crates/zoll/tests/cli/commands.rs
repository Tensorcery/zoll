//! The commands and their flags.

use crate::{Repo, TestResult, zoll};
use insta_cmd::assert_cmd_snapshot;

#[test]
fn help() {
    assert_cmd_snapshot!(zoll().arg("--help"));
}

#[test]
fn check_help() {
    assert_cmd_snapshot!(zoll().args(["check", "--help"]));
}

#[test]
fn version() {
    // The version a bump raises; the snapshot holds its place.
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(&crate::escape(env!("CARGO_PKG_VERSION")), "[VERSION]");
    settings.bind(|| assert_cmd_snapshot!(zoll().arg("--version")));
}

#[test]
fn no_command() {
    assert_cmd_snapshot!(zoll());
}

#[test]
fn unknown_format() {
    assert_cmd_snapshot!(zoll().args(["check", "--format", "xml"]));
}

#[test]
fn inspect_not_built() -> TestResult {
    let repo = Repo::new(&[])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").args(["inspect", "a.yml"])));
    Ok(())
}

#[test]
fn languages() {
    assert_cmd_snapshot!(zoll().arg("languages"));
}
