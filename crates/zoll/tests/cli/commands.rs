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
    assert_cmd_snapshot!(zoll().arg("--version"));
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
    assert_cmd_snapshot!(repo.zoll("").args(["inspect", "a.yml"]));
    Ok(())
}

#[test]
fn lock_not_built() -> TestResult {
    let repo = Repo::new(&[])?;
    assert_cmd_snapshot!(repo.zoll("").args(["lock", "--upgrade"]));
    Ok(())
}

#[test]
fn languages() {
    assert_cmd_snapshot!(zoll().arg("languages"));
}
