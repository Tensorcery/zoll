//! `zoll test`: the policies' tests, as `opa test` runs them.

use crate::{Repo, TestResult};
use insta_cmd::assert_cmd_snapshot;

const CONFIG: &str =
    "policies = [\"policy/\"]\ndata = [\"policy/data/\"]\n\n[packages]\nmain = \"yaml\"\n";

const POLICY: &str = r#"package main

deny contains "a Deployment names at least two replicas" if {
	input.kind == "Deployment"
	input.spec.replicas < data.limits.replicas
}
"#;

/// A repository with the policy above, its tests and its data.
fn repo(tests: &str) -> Result<Repo, Box<dyn std::error::Error>> {
    Repo::new(&[
        ("zoll.toml", CONFIG),
        ("policy/replicas.rego", POLICY),
        ("policy/replicas_test.rego", tests),
        ("policy/data/limits.json", "{\"replicas\": 2}\n"),
    ])
}

const TESTS: &str = r#"package main

test_one_replica_is_refused if {
	deny with input as {"kind": "Deployment", "spec": {"replicas": 1}}
}

test_two_pass if {
	count(deny) == 0 with input as {"kind": "Deployment", "spec": {"replicas": 2}}
}
"#;

#[test]
fn passing_tests() -> TestResult {
    let repo = repo(TESTS)?;
    assert_cmd_snapshot!(repo.zoll("").arg("test"));
    Ok(())
}

#[test]
fn a_failing_test_is_named() -> TestResult {
    let repo = repo(&format!(
        "{TESTS}\ntest_data_is_seen if {{\n\tdata.limits.replicas == 3\n}}\n"
    ))?;
    assert_cmd_snapshot!(repo.zoll("").arg("test"));
    Ok(())
}

#[test]
fn a_test_that_errors_fails_the_run() -> TestResult {
    let repo = repo(&format!(
        "{TESTS}\ntest_divides if {{\n\tx := 1 / 0\n\tx == 1\n}}\n"
    ))?;
    assert_cmd_snapshot!(repo.zoll("").arg("test"));
    Ok(())
}

#[test]
fn json() -> TestResult {
    let repo = repo(&format!(
        "{TESTS}\ntest_data_is_seen if {{\n\tdata.limits.replicas == 3\n}}\n"
    ))?;
    assert_cmd_snapshot!(repo.zoll("").args(["test", "--format", "json"]));
    Ok(())
}

#[test]
fn a_policy_that_does_not_load() -> TestResult {
    let repo = repo("package main\n\ntest_x if {\n")?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").arg("test")));
    Ok(())
}

#[test]
fn only_the_tests_under_a_path() -> TestResult {
    let repo = Repo::new(&[
        ("zoll.toml", CONFIG),
        ("policy/replicas.rego", POLICY),
        ("policy/replicas_test.rego", TESTS),
        ("policy/data/limits.json", "{\"replicas\": 2}\n"),
        (
            "policy/other/broken_test.rego",
            "package main\n\ntest_broken if {\n\tfalse\n}\n",
        ),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").args(["test", "policy/replicas_test.rego"]));
    Ok(())
}

#[test]
fn a_path_that_is_not_there() -> TestResult {
    let repo = repo(TESTS)?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").args(["test", "policy/nothing"])));
    Ok(())
}

#[test]
fn only_true_passes() -> TestResult {
    // As `conftest verify` runs them: a string, a number and false fail.
    let repo =
        repo("package main\n\ntest_string := \"yes\"\n\ntest_zero := 0\n\ntest_false := false\n")?;
    assert_cmd_snapshot!(repo.zoll("").arg("test"));
    Ok(())
}

#[test]
fn a_name_defined_twice_is_an_error() -> TestResult {
    // OPA runs each definition as a test; zoll cannot tell them apart.
    let repo = repo("package main\n\ntest_dup if {\n\ttrue\n}\n\ntest_dup if {\n\tfalse\n}\n")?;
    assert_cmd_snapshot!(repo.zoll("").arg("test"));
    Ok(())
}
