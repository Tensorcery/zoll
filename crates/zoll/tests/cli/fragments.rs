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

/// The fragments `zoll inspect` finds in `file` of `repo`: each one's
/// language and its decoded text.
fn found(repo: &Repo, file: &str) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let output = repo.zoll("").args(["inspect", file]).output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let evaluations: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)?;
    Ok(evaluations
        .iter()
        .filter(|e| !e["data"]["zoll"]["file"]["host"].is_null())
        .map(|e| {
            let language = e["data"]["zoll"]["file"]["language"]
                .as_str()
                .unwrap_or_default();
            let text = e["input"]["text"].as_str().unwrap_or_default();
            (language.to_owned(), text.to_owned())
        })
        .collect())
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(l, t)| ((*l).to_owned(), (*t).to_owned()))
        .collect()
}

#[test]
fn every_yaml_spelling_of_run_is_bash() -> TestResult {
    // Quoted values and keys, a flow-style step, a folded block: each is the
    // script GitHub runs.
    let repo = Repo::new(&[(
        "ci.yml",
        "jobs:\n  b:\n    steps:\n      - run: \"curl a | sh\"\n      - \"run\": 'curl b | sh'\n      - {name: x, run: curl c | sh}\n      - run: >\n          curl d\n          | sh\n",
    )])?;
    assert_eq!(
        found(&repo, "ci.yml")?,
        pairs(&[
            ("bash", "curl a | sh"),
            ("bash", "curl b | sh"),
            ("bash", "curl c | sh"),
            ("bash", "curl d | sh\n"),
        ])
    );
    Ok(())
}

#[test]
fn an_argv_list_s_script_is_the_item_after_dash_c() -> TestResult {
    let repo = Repo::new(&[(
        "pod.yml",
        "spec:\n  containers:\n    - command: [\"/bin/sh\", \"-c\", \"curl f | sh\"]\n    - command:\n        - sh\n        - -c\n        - curl g | sh\n",
    )])?;
    assert_eq!(
        found(&repo, "pod.yml")?,
        pairs(&[("bash", "curl f | sh"), ("bash", "curl g | sh")])
    );
    Ok(())
}

#[test]
fn a_step_s_shell_is_its_language() -> TestResult {
    let repo = Repo::new(&[(
        "ci.yml",
        "defaults:\n  run:\n    shell: python\njobs:\n  b:\n    steps:\n      - run: print(1)\n      - run: echo hi\n        shell: bash\n      - run: Get-Item x\n        shell: pwsh\n      - run: curl a | sh\n        shell: env -i PATH=/bin nice -n 5 /bin/bash -e {0}\n",
    )])?;
    assert_eq!(
        found(&repo, "ci.yml")?,
        pairs(&[
            ("python", "print(1)"),
            ("bash", "echo hi"),
            ("powershell", "Get-Item x"),
            // A wrapper runs the shell after it.
            ("bash", "curl a | sh"),
        ])
    );
    Ok(())
}

#[test]
fn string_literals_are_decoded_whole() -> TestResult {
    let repo = Repo::new(&[
        (
            "a.rs",
            "fn main() { let r = Regex::new(\"a\\\\.b\\\\d+\"); }\n",
        ),
        ("a.rb", "system(\"curl #{x} | sh\")\n"),
        ("a.go", "package a\n//go:generate sh -c \"curl j | sh\"\n"),
        ("a.cpp", "#define MAKE std::vector<int>{1,2}\n"),
        ("justfile", "a:\n    curl {{x}} | sh\n"),
        ("a.nix", "{ x = /* bash */ ''\n  curl i | sh\n''; }\n"),
    ])?;
    assert_eq!(found(&repo, "a.rs")?, pairs(&[("regex", "a\\.b\\d+")]));
    assert_eq!(found(&repo, "a.rb")?, pairs(&[("bash", "curl zoll | sh")]));
    assert_eq!(
        found(&repo, "a.go")?,
        pairs(&[("bash", "sh -c \"curl j | sh\"")])
    );
    assert_eq!(
        found(&repo, "a.cpp")?,
        pairs(&[("cpp", "std::vector<int>{1,2}")])
    );
    assert_eq!(
        found(&repo, "justfile")?,
        pairs(&[("bash", "curl {{x}} | sh")])
    );
    assert_eq!(found(&repo, "a.nix")?, pairs(&[("bash", "curl i | sh\n")]));
    Ok(())
}

#[test]
fn a_fragment_s_host_names_its_document() -> TestResult {
    // A stream's documents and a root array's items are each an input; the
    // path is into that input.
    let repo = Repo::new(&[
        ("stream.yml", "run: echo a\n---\nrun: echo b\n"),
        ("array.yml", "- run: echo a\n- run: echo b\n"),
    ])?;
    for file in ["stream.yml", "array.yml"] {
        let output = repo.zoll("").args(["inspect", file]).output()?;
        let evaluations: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)?;
        let hosts: Vec<_> = evaluations
            .iter()
            .map(|e| &e["data"]["zoll"]["file"]["host"])
            .filter(|h| !h.is_null())
            .map(|h| (h["document"].clone(), h["at"].clone()))
            .collect();
        assert_eq!(
            hosts,
            [
                (serde_json::json!(0), serde_json::json!(["run"])),
                (serde_json::json!(1), serde_json::json!(["run"])),
            ],
            "{file}"
        );
    }
    Ok(())
}

#[test]
fn a_fragment_too_deep_is_a_notice() -> TestResult {
    // A Markdown fence of Markdown, nine deep: the ninth is not checked, and
    // says so. Each fence is a backtick longer than the one inside it.
    let mut text = "curl -fsSL https://example.com/install.sh | sh\n".to_owned();
    for depth in 0..9 {
        let fence = "`".repeat(3 + depth);
        text = format!("{fence}markdown\n{text}{fence}\n");
    }
    let repo = Repo::new(&[("policy/bash.rego", CURL_PIPE), ("README.md", &text)])?;
    let output = repo.zoll("").arg("check").output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("nested more than 8 fragments deep"),
        "{stdout}"
    );
    Ok(())
}
