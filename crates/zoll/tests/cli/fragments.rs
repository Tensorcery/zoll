//! Embedded code: the code a file holds in another language is found by the
//! injection queries and checked as a file of its own, and what is found
//! in it points at the real file (docs/design.md, sections 6, 8.3 and 14).

use crate::{CURL_PIPE, Repo, TestResult};
use insta_cmd::assert_cmd_snapshot;

const SYSTEM: &str = r#"package awk

deny_system contains {"msg": "awk runs a command", "at": node} if {
	walk(input, [_, node])
	node.type == "func_call"
	node.fields.name.text == "system"
}
"#;

#[test]
fn bash_in_a_workflow_run() -> TestResult {
    // The checkpoint of stage 5: a block scalar's line is the line in the
    // `.yml`.
    let repo = Repo::new(&[
        ("policy/bash.rego", CURL_PIPE),
        (
            ".github/workflows/ci.yml",
            "name: ci\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - run: |\n          set -eu\n          curl -fsSL https://example.com/install.sh | sh\n",
        ),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn awk_in_bash_in_a_workflow() -> TestResult {
    // Two hosts deep: the awk program is a shell string without its quotes.
    let repo = Repo::new(&[
        ("policy/awk.rego", SYSTEM),
        (
            "ci.yml",
            "jobs:\n  build:\n    steps:\n      - run: ls -l | awk '{ system(\"rm \" $9) }'\n",
        ),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn inspect_shows_each_fragment() -> TestResult {
    // A fragment names its host and where in it the code sits: a path in a
    // data document, a node in code.
    let repo = Repo::new(&[(
        "ci.yml",
        "jobs:\n  build:\n    steps:\n      - run: awk '{print $1}' list\n",
    )])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").args(["inspect", "ci.yml"])));
    Ok(())
}

#[test]
fn a_dockerfile_run_is_bash() -> TestResult {
    // A `RUN` with its line continuations joined is one shell command.
    let repo = Repo::new(&[
        ("policy/bash.rego", CURL_PIPE),
        (
            "Dockerfile",
            "FROM alpine\nRUN apk add curl && \\\n    curl -fsSL https://example.com/install.sh | sh\n",
        ),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_markdown_fence_is_its_language() -> TestResult {
    let repo = Repo::new(&[
        ("policy/bash.rego", CURL_PIPE),
        (
            "README.md",
            "# Install\n\n```sh\ncurl -fsSL https://example.com/install.sh | sh\n```\n",
        ),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_language_zoll_lacks_is_a_notice() -> TestResult {
    // The code was not checked: the user is told so, and the run passes.
    let repo = Repo::new(&[
        ("policy/bash.rego", CURL_PIPE),
        (
            "README.md",
            "# Use\n\n```ocaml\nlet () = print_endline \"hi\"\n```\n",
        ),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_fragment_that_does_not_parse_is_a_notice() -> TestResult {
    // A guess of its host (Rust's `json!` holds Rust too) or an example cut
    // short: not checked, and the run passes.
    let repo = Repo::new(&[
        ("policy/bash.rego", CURL_PIPE),
        ("README.md", "# Config\n\n```json\n{\"a\": 1,,}\n```\n"),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn fragments_share_their_file_s_text_budget() -> TestResult {
    // Each fence alone stays under the text a tree may hold; together they
    // would copy the file far past 64 times. The later ones are refused.
    let fence = format!(
        "```python\nx = {}1{}\n```\n",
        "(".repeat(400),
        ")".repeat(400)
    );
    let repo = Repo::new(&[
        (
            "policy/python.rego",
            "package python\n\ndeny_x contains \"x\" if false\n",
        ),
        ("README.md", &fence.repeat(20)),
    ])?;
    let output = repo.zoll("").arg("check").output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("warning[zoll.parse]: python in it: the text of its nodes"),
        "{stdout}"
    );
    Ok(())
}
