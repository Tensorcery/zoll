//! `zoll inspect FILE`: the documents a policy is given for a file.

use crate::{Repo, TestResult};
use insta_cmd::assert_cmd_snapshot;

fn inspect(repo: &Repo, file: &str) -> std::process::Command {
    let mut command = repo.zoll("");
    command.args(["inspect", file]);
    command
}

#[test]
fn yaml_scalars_are_yaml_1_2() -> TestResult {
    let repo = Repo::new(&[(
        "a.yml",
        r#"on: push
yes: no
off: y
t: true
T: True
quoted: "true"
tilde: ~
null_word: null
empty:
int: 42
negative: -7
octal: 0o17
hex: 0x1F
leading_zero: 017
float: 1.5
exponent: 1e3
version: 1.10
date: 2024-01-01
str_tag: !!str 42
int_tag: !!int "42"
custom_tag: !ref value
"#,
    )])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.yml")));
    Ok(())
}

#[test]
fn yaml_block_scalars() -> TestResult {
    let repo = Repo::new(&[(
        "a.yml",
        "literal: |\n  one\n  two\nfolded: >\n  one\n  two\nstrip: |-\n  x\nkeep: |+\n  x\n\n",
    )])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.yml")));
    Ok(())
}

#[test]
fn yaml_anchors_and_merge_keys() -> TestResult {
    let repo = Repo::new(&[(
        "a.yml",
        "base: &base {a: 1, b: 2}\nmore: &more {c: 3}\nlist: &list [x, y]\none:\n  <<: *base\n  b: 20\nboth:\n  <<: [*base, *more]\n  a: 10\nalias: *list\n",
    )])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.yml")));
    Ok(())
}

#[test]
fn yaml_keys_become_strings() -> TestResult {
    let repo = Repo::new(&[("a.yml", "1: int\n1.5: float\ntrue: bool\na: 1\na: 2\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.yml")));
    Ok(())
}

#[test]
fn yaml_documents_one_by_one() -> TestResult {
    let repo = Repo::new(&[("a.yml", "---\nkind: A\n---\nkind: B\n...\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.yml")));
    Ok(())
}

#[test]
fn root_array_one_by_one() -> TestResult {
    let repo = Repo::new(&[("a.json", "[{\"n\": 1}, {\"n\": 2}]")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.json")));
    Ok(())
}

#[test]
fn toml_dates_as_written() -> TestResult {
    let repo = Repo::new(&[(
        "a.toml",
        "offset = 1979-05-27T07:32:00Z\nlocal = 1979-05-27T07:32:00\nday = 1979-05-27\ntime = 07:32:00\n",
    )])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.toml")));
    Ok(())
}

#[test]
fn path_relative_to_the_config() -> TestResult {
    let repo = Repo::new(&[("zoll.toml", ""), ("sub/a.yml", "a: 1\n")])?;
    repo.settings()?.bind(|| {
        assert_cmd_snapshot!(repo.zoll("sub").args(["inspect", "a.yml"]));
    });
    Ok(())
}

#[test]
fn language_from_the_config() -> TestResult {
    let repo = Repo::new(&[
        ("zoll.toml", "[languages]\n\"*.conf\" = \"yaml\"\n"),
        ("a.conf", "a: 1\n"),
    ])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.conf")));
    Ok(())
}

// What a file zoll cannot give a document says.

#[test]
fn yaml_error() -> TestResult {
    let repo = Repo::new(&[("a.yml", "a: 1\nb: [1, 2\nc: 3\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.yml")));
    Ok(())
}

#[test]
fn yaml_infinity() -> TestResult {
    let repo = Repo::new(&[("a.yml", "a: 1\nb: .inf\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.yml")));
    Ok(())
}

#[test]
fn yaml_null_key() -> TestResult {
    let repo = Repo::new(&[("a.yml", "a: 1\n~: 2\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.yml")));
    Ok(())
}

#[test]
fn json_error() -> TestResult {
    let repo = Repo::new(&[("a.json", "{\n  \"a\": 1,\n}\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.json")));
    Ok(())
}

#[test]
fn toml_error() -> TestResult {
    let repo = Repo::new(&[("a.toml", "a = 1\na = 2\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.toml")));
    Ok(())
}

#[test]
fn no_language() -> TestResult {
    let repo = Repo::new(&[("notes.txt", "hi\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "notes.txt")));
    Ok(())
}

#[test]
fn missing_file() -> TestResult {
    let repo = Repo::new(&[])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(inspect(&repo, "a.yml")));
    Ok(())
}

#[test]
fn reader_that_stops_reading() -> TestResult {
    // More than a pipe holds, so zoll is still writing when the pipe closes.
    let yaml = "- item\n".repeat(20_000);
    let repo = Repo::new(&[("a.yml", &yaml)])?;
    let mut child = inspect(&repo, "a.yml")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    drop(child.stdout.take());
    let output = child.wait_with_output()?;
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert!(output.status.success());
    Ok(())
}

#[test]
fn nested_anchors_count_against_the_limit() -> TestResult {
    // Each anchor keeps a copy of everything under it: 200 of them over a
    // list of 6000 copy 1.2 million nodes from a file of 6200. (The parser
    // refuses nesting much deeper than that by itself.)
    let depth = 200;
    let mut yaml = String::new();
    for level in 0..depth {
        yaml += "&a";
        yaml += &level.to_string();
        yaml += " [";
    }
    yaml += &vec!["x"; 6000].join(", ");
    yaml += &"]".repeat(depth);
    let repo = Repo::new(&[("a.yml", &yaml)])?;
    let output = inspect(&repo, "a.yml").output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("more than a million nodes"), "{stderr}");
    Ok(())
}
