//! Code: a file of a language with a grammar becomes its tree-sitter tree
//! (docs/design.md, section 8.2), and policies run on it.

use crate::{Repo, TestResult};
use insta_cmd::assert_cmd_snapshot;

#[test]
fn inspect_bash() -> TestResult {
    let repo = Repo::new(&[("run.sh", "ls | awk '{print $1}'\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").args(["inspect", "run.sh"])));
    Ok(())
}

#[test]
fn a_field_of_several_is_always_a_list() -> TestResult {
    // `argument` may repeat in bash's grammar: a list even with one; `name`
    // may not: an object.
    let repo = Repo::new(&[("run.sh", "echo hi\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").args(["inspect", "run.sh"])));
    Ok(())
}

#[test]
fn columns_count_characters() -> TestResult {
    let repo = Repo::new(&[("a.py", "s = \"ёж\"; n = 1\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").args(["inspect", "a.py"])));
    Ok(())
}

#[test]
fn errors_stay_in_the_tree() -> TestResult {
    let repo = Repo::new(&[("bad.sh", "if then\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").args(["inspect", "bad.sh"])));
    Ok(())
}

#[test]
fn languages_compiled_in() {
    assert_cmd_snapshot!(crate::zoll().arg("languages"));
}

const CURL_PIPE: &str = r#"package bash

# A pipe into a shell downloads and runs code nobody reviewed.
deny_curl_pipe contains {"msg": "curl piped into a shell", "at": node} if {
	walk(input, [_, node])
	node.type == "pipeline"
	some cmd in node.children
	cmd.type == "command"
	cmd.fields.name.children[0].text == "curl"
}
"#;

#[test]
fn a_policy_points_at_a_node() -> TestResult {
    let repo = Repo::new(&[
        ("policy/bash.rego", CURL_PIPE),
        (
            "install.sh",
            "#!/bin/sh\nset -eu\n\n  curl -fsSL https://example.com/install.sh | sh\necho done\n",
        ),
        (
            "safe.sh",
            "curl -fsSL -o install.sh https://example.com/install.sh\n",
        ),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_policy_on_python() -> TestResult {
    let repo = Repo::new(&[
        (
            "policy/python.rego",
            "package python\n\ndeny_eval contains {\"msg\": \"eval runs a string\", \"at\": node} if {\n\twalk(input, [_, node])\n\tnode.type == \"call\"\n\tnode.fields.function.text == \"eval\"\n}\n",
        ),
        ("a.py", "x = 1\ny = eval(input())\n"),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_tree_too_deep_is_a_warning() -> TestResult {
    // Walked deeper, the tree would overflow the stack: zoll refuses it.
    let deep = format!("x = {}1{}\n", "(".repeat(600), ")".repeat(600));
    let repo = Repo::new(&[
        (
            "policy/python.rego",
            "package python\n\ndeny_x contains \"x\" if false\n",
        ),
        ("a.py", &deep),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}
