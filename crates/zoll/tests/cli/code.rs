//! Code: a file of a language with a grammar becomes its tree-sitter tree
//! (docs/design.md, section 8.2), and policies run on it.

use crate::{CURL_PIPE, Repo, TestResult};
use insta_cmd::assert_cmd_snapshot;

#[test]
fn inspect_bash() -> TestResult {
    let repo = Repo::new(&[("run.sh", "ls | awk '{print $1}'\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").args(["inspect", "run.sh"])));
    Ok(())
}

#[test]
fn every_node_has_its_text() -> TestResult {
    // A policy reads a node's text without knowing how its grammar nests
    // it: bash's `command_name` holds a `word`, Python's `string` three
    // parts.
    let repo = Repo::new(&[
        (
            "policy/python.rego",
            "package python\n\ndeny_secret contains {\"msg\": \"a password in the code\", \"at\": node} if {\n\twalk(input, [_, node])\n\tnode.type == \"string\"\n\tnode.text == \"\\\"hunter2\\\"\"\n}\n",
        ),
        ("a.py", "user = \"admin\"\npassword = \"hunter2\"\n"),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
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

/// A Python file of `lines` lines, each `x = (((1)))` nested `depth` deep,
/// with a Python policy to check it.
fn nested(depth: usize, lines: usize) -> Result<Repo, Box<dyn std::error::Error>> {
    let line = format!("x = {}1{}\n", "(".repeat(depth), ")".repeat(depth));
    Repo::new(&[
        (
            "policy/python.rego",
            "package python\n\ndeny_x contains \"x\" if false\n",
        ),
        ("a.py", &line.repeat(lines)),
    ])
}

#[test]
fn a_tree_too_deep_is_a_warning() -> TestResult {
    // Walked deeper, the tree would overflow the stack: zoll refuses it.
    let repo = nested(600, 1)?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_tree_with_too_much_text_is_a_warning() -> TestResult {
    // Each node repeats the text under it: lines of 400 nested parentheses
    // would copy the file hundreds of times over. zoll refuses it.
    let repo = nested(400, 20)?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}
