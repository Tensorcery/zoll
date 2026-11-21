//! The reports other than `text`: `json`, `github` and `sarif`
//! (docs/design.md, section 12).

use insta_cmd::assert_cmd_snapshot;

use crate::{CURL_PIPE, Repo, TestResult};

/// A policy on awk, for a fragment two deep.
const AWK_SYSTEM: &str = r#"package awk

deny_system contains {"msg": "awk runs a command", "at": node} if {
	walk(input, [_, node])
	node.type == "func_call"
	node.fields.name.text == "system"
}
"#;

/// A warning whose message GitHub must escape, with metadata.
const ECHO: &str = r#"package bash

warn_echo contains {"msg": "echo: say it,\nonce", "at": node, "url": "https://example.com"} if {
	walk(input, [_, node])
	node.type == "command"
	node.fields.name.text == "echo"
}
"#;

/// A shell script, and awk in bash in a workflow.
fn repo() -> Result<Repo, Box<dyn std::error::Error>> {
    Repo::new(&[
        ("policy/bash.rego", CURL_PIPE),
        ("policy/echo.rego", ECHO),
        ("policy/awk.rego", AWK_SYSTEM),
        (
            "a b.sh",
            "echo hi\ncurl -fsSL https://example.com/install.sh | sh\n",
        ),
        (
            "ci.yml",
            "jobs:\n  build:\n    steps:\n      - run: ls -l | awk '{ system(\"rm \" $9) }'\n",
        ),
    ])
}

/// Snapshot settings that write the version as `[VERSION]`.
fn settings(repo: &Repo) -> Result<insta::Settings, Box<dyn std::error::Error>> {
    let mut settings = repo.settings()?;
    settings.add_filter(&regex_escape(env!("CARGO_PKG_VERSION")), "[VERSION]");
    Ok(settings)
}

fn regex_escape(text: &str) -> String {
    text.replace('.', r"\.")
}

#[test]
fn json_has_each_result_and_its_fragments() -> TestResult {
    let repo = repo()?;
    assert_cmd_snapshot!(repo.zoll("").args(["check", "--format", "json"]));
    Ok(())
}

#[test]
fn github_writes_workflow_commands() -> TestResult {
    let repo = repo()?;
    assert_cmd_snapshot!(repo.zoll("").args(["check", "--format", "github"]));
    Ok(())
}

#[test]
fn sarif_is_one_run_with_its_rules() -> TestResult {
    let repo = repo()?;
    settings(&repo)?.bind(|| {
        assert_cmd_snapshot!(repo.zoll("").args(["check", "--format", "sarif"]));
    });
    Ok(())
}

#[test]
fn nothing_found_is_an_empty_report() -> TestResult {
    let repo = Repo::new(&[("policy/bash.rego", CURL_PIPE), ("a.sh", "echo hi\n")])?;
    for format in ["json", "github"] {
        let output = repo.zoll("").args(["check", "--format", format]).output()?;
        assert!(output.status.success());
        let expected = if format == "json" { "[]\n" } else { "" };
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{format}"
        );
    }
    Ok(())
}

#[test]
fn a_file_outside_the_directory_is_a_file_uri() -> TestResult {
    let repo = repo()?;
    // Found from `policy/`, up.
    std::fs::write(repo.path().join("zoll.toml"), "")?;
    let output = repo
        .zoll("")
        .args(["check", "--format", "sarif"])
        .arg(repo.path().join("a b.sh"))
        .current_dir(repo.path().join("policy"))
        .output()?;
    let sarif: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let uri = &sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]
        ["uri"];
    let path = repo.path().canonicalize()?.join("a%20b.sh");
    assert_eq!(
        uri.as_str(),
        Some(format!("file://{}", path.display()).as_str())
    );
    Ok(())
}

#[test]
fn a_reader_that_stops_keeps_the_exit_code() -> TestResult {
    let repo = repo()?;
    let mut child = repo
        .zoll("")
        .arg("check")
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    // Gone before zoll writes a byte.
    drop(child.stdout.take());
    assert_eq!(child.wait()?.code(), Some(1));
    Ok(())
}
