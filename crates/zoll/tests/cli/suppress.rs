//! Suppression: `per-file-ignores` in the config, `zoll: ignore[...]` on the
//! line, and each one that silences nothing reported (docs/design.md,
//! section 9).

use insta_cmd::assert_cmd_snapshot;

use crate::{CURL_PIPE, Repo, TestResult};

const CURL: &str = "curl -fsSL https://example.com/install.sh | sh";

#[test]
fn a_comment_on_the_line_silences_its_rule() -> TestResult {
    let repo = Repo::new(&[
        ("policy/bash.rego", CURL_PIPE),
        ("a.sh", &format!("{CURL}  # zoll: ignore[bash.curl_pipe]\n")),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_comment_in_a_workflow_run_silences_the_bash_in_it() -> TestResult {
    // The comment is bash's and YAML's `#` alike, on the reported line.
    let repo = Repo::new(&[
        ("policy/bash.rego", CURL_PIPE),
        (
            "ci.yml",
            &format!(
                "jobs:\n  build:\n    steps:\n      - run: |\n          set -eu\n          {CURL}  # zoll: ignore[bash.curl_pipe]\n"
            ),
        ),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_comment_silences_its_own_line_only() -> TestResult {
    let repo = Repo::new(&[
        ("policy/bash.rego", CURL_PIPE),
        ("a.sh", &format!("# zoll: ignore[bash.curl_pipe]\n{CURL}\n")),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn per_file_ignores_silence_a_rule_in_the_files_matched() -> TestResult {
    let repo = Repo::new(&[
        (
            "zoll.toml",
            "[per-file-ignores]\n\"scripts/legacy/**\" = [\"bash.curl_pipe\"]\n",
        ),
        ("policy/bash.rego", CURL_PIPE),
        ("scripts/legacy/old.sh", &format!("{CURL}\n")),
        ("scripts/new.sh", &format!("{CURL}\n")),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn per_file_ignores_silence_a_template_that_does_not_parse() -> TestResult {
    let repo = Repo::new(&[
        (
            "zoll.toml",
            "[per-file-ignores]\n\"charts/**/templates/**\" = [\"zoll.parse\"]\n",
        ),
        ("policy/bash.rego", CURL_PIPE),
        (
            "charts/app/templates/deploy.yaml",
            "metadata:\n  name: {{ .Release.Name }\n",
        ),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_comment_that_silences_nothing_is_a_warning() -> TestResult {
    let repo = Repo::new(&[
        ("policy/bash.rego", CURL_PIPE),
        ("a.sh", "echo hi  # zoll: ignore[bash.curl_pipe]\n"),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_per_file_ignore_that_silences_nothing_is_a_warning() -> TestResult {
    let repo = Repo::new(&[
        (
            "zoll.toml",
            "[per-file-ignores]\n\"scripts/**\" = [\"bash.curl_pipe\", \"bash.typo\"]\n",
        ),
        ("policy/bash.rego", CURL_PIPE),
        ("scripts/a.sh", &format!("{CURL}\n")),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_run_over_some_files_does_not_judge_per_file_ignores() -> TestResult {
    // A pre-commit hook checks the files changed: it has not seen the files
    // a glob was written for.
    let repo = Repo::new(&[
        (
            "zoll.toml",
            "[per-file-ignores]\n\"scripts/**\" = [\"bash.curl_pipe\"]\n",
        ),
        ("policy/bash.rego", CURL_PIPE),
        ("scripts/a.sh", &format!("{CURL}\n")),
        ("b.sh", "echo hi\n"),
    ])?;
    assert_cmd_snapshot!(repo.zoll("").args(["check", "b.sh"]));
    Ok(())
}
