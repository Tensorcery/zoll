//! `zoll check` with policies: which run where, what a rule yields, where a
//! violation points, and what the run exits with.

use crate::{Repo, TestResult};
use insta_cmd::assert_cmd_snapshot;

/// A repository with these policies under `policy/`, these files, and no
/// config unless one is among the files.
fn repo(
    policies: &[(&str, &str)],
    files: &[(&str, &str)],
) -> Result<Repo, Box<dyn std::error::Error>> {
    let policies: Vec<(String, &str)> = policies
        .iter()
        .map(|(name, text)| (format!("policy/{name}"), *text))
        .collect();
    let mut all: Vec<(&str, &str)> = policies.iter().map(|(p, t)| (p.as_str(), *t)).collect();
    all.extend_from_slice(files);
    Repo::new(&all)
}

const WORKFLOW: &str = "name: CI
on: push
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - run: |
          curl -fsSL https://example.com/install.sh | sh
";

#[test]
fn a_string_points_at_the_document() -> TestResult {
    let repo = repo(
        &[(
            "ci.rego",
            "package yaml\n\ndeny contains \"no workflow may run curl\" if input.name == \"CI\"\n",
        )],
        &[("ci.yml", WORKFLOW)],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn at_a_path_points_at_its_key() -> TestResult {
    let repo = repo(
        &[(
            "ci.rego",
            r#"package yaml

deny_curl_pipe contains {"msg": "curl piped into a shell", "at": ["jobs", name, "steps", i, "run"]} if {
	some name, job in input.jobs
	some i, step in job.steps
	contains(step.run, "| sh")
}
"#,
        )],
        &[("ci.yml", WORKFLOW)],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn at_a_path_in_json_and_toml() -> TestResult {
    let repo = repo(
        &[
            (
                "json.rego",
                "package json\n\ndeny_private contains {\"msg\": \"published\", \"at\": [\"private\"]} if input.private == false\n",
            ),
            (
                "toml.rego",
                "package toml\n\ndeny_lto contains {\"msg\": \"no lto\", \"at\": [\"profile\", \"release\", \"lto\"]} if input.profile.release.lto == false\n",
            ),
        ],
        &[
            (
                "package.json",
                "{\n  \"name\": \"x\",\n  \"private\": false\n}\n",
            ),
            (
                "Cargo.toml",
                "[package]\nname = \"x\"\n\n[profile.release]\nlto = false\n",
            ),
        ],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn at_a_path_not_in_the_document_points_at_what_is() -> TestResult {
    let repo = repo(
        &[(
            "ci.rego",
            "package yaml\n\ndeny contains {\"msg\": \"no timeout\", \"at\": [\"jobs\", \"build\", \"timeout-minutes\"]} if true\n",
        )],
        &[("ci.yml", WORKFLOW)],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn each_document_and_each_item_points_at_itself() -> TestResult {
    let repo = repo(
        &[
            (
                "kind.rego",
                "package yaml\n\ndeny contains sprintf(\"a %s\", [input.kind]) if input.kind == \"Secret\"\n",
            ),
            (
                "list.rego",
                "package json\n\ndeny contains {\"msg\": \"disabled\", \"at\": [\"enabled\"]} if input.enabled == false\n",
            ),
        ],
        &[
            (
                "k8s.yaml",
                "kind: ConfigMap\n---\nkind: Secret\n---\nkind: Secret\n",
            ),
            (
                "rules.json",
                "[\n  {\"enabled\": true},\n  {\"enabled\": false}\n]\n",
            ),
        ],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn loc_names_the_line() -> TestResult {
    let repo = repo(
        &[(
            "ci.rego",
            "package yaml\n\ndeny contains {\"msg\": \"here\", \"_loc\": {\"file\": \"ci.yml\", \"line\": 7}} if true\n",
        )],
        &[("ci.yml", WORKFLOW)],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

// Rule names, ids and severities.

#[test]
fn rule_ids_and_severities() -> TestResult {
    let repo = repo(
        &[(
            "ids.rego",
            "package yaml\n\ndeny contains \"bare\" if true\ndeny_curl_pipe contains \"deny\" if true\nviolation_two_words contains \"violation\" if true\nwarn contains \"bare warn\" if true\nwarn_style contains \"warn\" if true\nhelper contains \"not a rule zoll runs\" if true\n",
        )],
        &[("a.yml", "a: 1\n")],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn warnings_alone_pass() -> TestResult {
    let repo = repo(
        &[(
            "w.rego",
            "package yaml\n\nwarn_style contains \"style\" if true\n",
        )],
        &[("a.yml", "a: 1\n")],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn fail_on_warn_flag() -> TestResult {
    let repo = repo(
        &[(
            "w.rego",
            "package yaml\n\nwarn_style contains \"style\" if true\n",
        )],
        &[("a.yml", "a: 1\n")],
    )?;
    assert_cmd_snapshot!(repo.zoll("").args(["check", "--fail-on", "warn"]));
    Ok(())
}

#[test]
fn fail_on_warn_config() -> TestResult {
    let repo = repo(
        &[(
            "w.rego",
            "package yaml\n\nwarn_style contains \"style\" if true\n",
        )],
        &[("a.yml", "a: 1\n"), ("zoll.toml", "fail-on = \"warn\"\n")],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn no_violation() -> TestResult {
    let repo = repo(
        &[(
            "ok.rego",
            "package yaml\n\ndeny contains \"never\" if false\n",
        )],
        &[("a.yml", "a: 1\n")],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn exceptions_silence_a_rule() -> TestResult {
    let repo = repo(
        &[(
            "e.rego",
            "package yaml\n\ndeny_curl_pipe contains \"silenced\" if true\ndeny_other contains \"kept\" if true\ndeny contains \"bare, silenced\" if true\nexception contains [\"curl_pipe\", \"\"] if true\n",
        )],
        &[("a.yml", "a: 1\n")],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

// Which package runs on which language.

#[test]
fn package_main_is_refused() -> TestResult {
    let repo = repo(
        &[("main.rego", "package main\n\ndeny contains \"x\" if true\n")],
        &[("a.yml", "a: 1\n")],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn package_bound_by_the_config() -> TestResult {
    let repo = repo(
        &[(
            "main.rego",
            "package main\n\ndeny_x contains \"main on yaml\" if input.a == 1\n",
        )],
        &[
            ("a.yml", "a: 1\n"),
            ("b.json", "{\"a\": 1}"),
            ("zoll.toml", "[packages]\nmain = \"yaml\"\n"),
        ],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn package_bound_to_no_language() -> TestResult {
    let repo = repo(
        &[("main.rego", "package main\n\ndeny contains \"x\" if true\n")],
        &[
            ("a.yml", "a: 1\n"),
            ("zoll.toml", "[packages]\nmain = \"yml\"\n"),
        ],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_language_runs_on_its_own_files_only() -> TestResult {
    let repo = repo(
        &[(
            "json.rego",
            "package json\n\ndeny contains \"json only\" if true\n",
        )],
        &[("a.yml", "a: 1\n"), ("b.json", "{}"), ("c.sh", "echo\n")],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_library_is_imported() -> TestResult {
    let repo = repo(
        &[
            (
                "lib.rego",
                "package lib.text\n\nshout(s) := upper(s)\n\ndeny contains \"a library is never run\" if true\n",
            ),
            (
                "a.rego",
                "package yaml\n\nimport data.lib.text\n\ndeny contains text.shout(input.a) if true\n",
            ),
        ],
        &[("a.yml", "a: quiet\n")],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

// What a policy is given.

#[test]
fn data_directories() -> TestResult {
    let repo = repo(
        &[(
            "d.rego",
            "package yaml\n\ndeny contains sprintf(\"%v %v\", [data.limit, data.teams.owner]) if data.zoll.file.path == \"a.yml\"\n",
        )],
        &[
            ("a.yml", "a: 1\n"),
            ("policy/data/limits.json", "{\"limit\": 3}"),
            ("policy/data/teams/owners.yaml", "owner: platform\n"),
            ("zoll.toml", "data = [\"policy/data\"]\n"),
        ],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn rego_v0_when_asked() -> TestResult {
    let v0 = "package yaml\n\ndeny[msg] {\n\tinput.a == 1\n\tmsg := \"written in v0\"\n}\n";
    let repo = repo(
        &[("v0.rego", v0)],
        &[
            ("a.yml", "a: 1\n"),
            ("zoll.toml", "rego-version = \"v0\"\n"),
        ],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn rego_v1_by_default() -> TestResult {
    let v0 = "package yaml\n\ndeny[msg] {\n\tinput.a == 1\n\tmsg := \"written in v0\"\n}\n";
    let repo = repo(&[("v0.rego", v0)], &[("a.yml", "a: 1\n")])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn stdin_is_yaml() -> TestResult {
    let repo = repo(
        &[(
            "k.rego",
            "package yaml\n\ndeny contains input.kind if true\n",
        )],
        &[],
    )?;
    assert_cmd_snapshot!(
        repo.zoll("")
            .args(["check", "-"])
            .pass_stdin("kind: Deployment\n---\nkind: Service\n")
    );
    Ok(())
}

// What is not right in the policies or the files.

#[test]
fn a_result_without_msg() -> TestResult {
    let repo = repo(
        &[(
            "m.rego",
            "package yaml\n\ndeny_x contains {\"message\": \"typo\"} if true\n",
        )],
        &[("a.yml", "a: 1\n")],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_rule_that_is_not_a_set() -> TestResult {
    let repo = repo(
        &[("b.rego", "package yaml\n\ndeny if input.a == 1\n")],
        &[("a.yml", "a: 1\n")],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_policy_that_does_not_parse() -> TestResult {
    let repo = repo(
        &[("bad.rego", "package yaml\n\ndeny contains \"x\" if {\n")],
        &[("a.yml", "a: 1\n")],
    )?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").arg("check")));
    Ok(())
}

#[test]
fn no_policies() -> TestResult {
    let repo = Repo::new(&[("a.yml", "a: 1\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").arg("check")));
    Ok(())
}

#[test]
fn a_file_that_does_not_parse_is_a_warning() -> TestResult {
    let repo = repo(
        &[(
            "ok.rego",
            "package yaml\n\ndeny contains \"never\" if false\n",
        )],
        &[(
            "templates/deploy.yaml",
            "metadata:\n  name: {{ .Release.Name }}\n  labels: [a\n",
        )],
    )?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn a_file_not_utf8_fails_the_run() -> TestResult {
    let repo = Repo::new(&[
        ("policy/bash.rego", crate::CURL_PIPE),
        ("b.sh", "curl -fsSL https://example.com/x | sh\n"),
    ])?;
    // `é` in Latin-1.
    std::fs::write(repo.path().join("a.sh"), b"echo caf\xe9\n")?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn places_past_an_escaped_emoji_and_a_bom() -> TestResult {
    let repo = repo(
        &[
            (
                "json.rego",
                "package json\n\ndeny contains {\"msg\": \"b\", \"at\": [\"b\"]} if input.b\n",
            ),
            (
                "toml.rego",
                "package toml\n\ndeny contains {\"msg\": \"k\", \"at\": [\"k\"]} if input.k\n",
            ),
        ],
        &[
            ("a.json", "{\"a\": \"\\ud83d\\ude00\", \"b\": 1}\n"),
            ("a.toml", "\u{feff}k = 1\n"),
        ],
    )?;
    let stdout = String::from_utf8(repo.zoll("").arg("check").output()?.stdout)?;
    assert!(stdout.contains("--> a.json:1:23"), "{stdout}");
    assert!(stdout.contains("--> a.toml:1:2"), "{stdout}");
    Ok(())
}

#[test]
fn a_file_too_big_fails_the_run_unless_ignored() -> TestResult {
    let repo = repo(&[("bash.rego", crate::CURL_PIPE)], &[])?;
    std::fs::write(repo.path().join("big.json"), vec![b' '; 17 << 20])?;
    let output = repo.zoll("").arg("check").output()?;
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("is more than 16 MiB: not read"), "{stdout}");
    // Passed on purpose.
    std::fs::write(
        repo.path().join("zoll.toml"),
        "[per-file-ignores]\n\"big.json\" = [\"zoll.parse\"]\n",
    )?;
    assert!(repo.zoll("").arg("check").output()?.status.success());
    Ok(())
}
