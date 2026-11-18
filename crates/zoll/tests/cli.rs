//! The command as a user runs it: the binary, its arguments, its output.

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::process::Command;

fn zoll() -> Command {
    Command::new(get_cargo_bin("zoll"))
}

#[test]
fn help() {
    assert_cmd_snapshot!(zoll().arg("--help"));
}
