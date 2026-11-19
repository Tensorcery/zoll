//! Where the config is found, and what a broken one says.

use crate::{Repo, TestResult};
use insta_cmd::assert_cmd_snapshot;

const VALID: &str = r#"
include = ["**/*"]
exclude = ["vendor/**"]
policies = ["policy/", "git+https://github.com/Tensorcery/zoll-policies//embedded-code@v1.0.0"]
data = ["policy/data/"]
fail-on = "warn"
rego-version = "v0"

[packages]
main = "yaml"

[languages]
"Jenkinsfile" = "groovy"

[per-file-ignores]
"scripts/legacy/**" = ["bash.curl_pipe"]
"#;

// A valid config is read whole; `--show-files` stops there, before the
// policies would load.

#[test]
fn check_without_config() -> TestResult {
    let repo = Repo::new(&[("a.yml", "")])?;
    assert_cmd_snapshot!(repo.zoll("").args(["check", "--show-files"]));
    Ok(())
}

#[test]
fn check_every_field() -> TestResult {
    let repo = Repo::new(&[("zoll.toml", VALID), ("src/a.yml", "")])?;
    assert_cmd_snapshot!(repo.zoll("").args([
        "check",
        "src",
        "--format",
        "json",
        "--locked",
        "--show-files"
    ]));
    Ok(())
}

#[test]
fn check_pyproject_every_field() -> TestResult {
    let pyproject = format!(
        "[project]\nname = \"x\"\n\n[tool.zoll]\n{}",
        VALID.replace("\n[", "\n[tool.zoll.")
    );
    let repo = Repo::new(&[("pyproject.toml", &pyproject)])?;
    assert_cmd_snapshot!(repo.zoll("").args(["check", "--show-files"]));
    Ok(())
}

// Where the config is found. A broken one names itself, so the error shows
// which file was taken.

#[test]
fn found_in_a_parent() -> TestResult {
    let repo = Repo::new(&[("zoll.toml", "includ = []"), ("a/b/.keep", "")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("a/b").arg("check")));
    Ok(())
}

#[test]
fn dotted_name_first() -> TestResult {
    let repo = Repo::new(&[(".zoll.toml", "dotted = 1"), ("zoll.toml", "plain = 1")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").arg("check")));
    Ok(())
}

#[test]
fn zoll_toml_before_pyproject() -> TestResult {
    let repo = Repo::new(&[
        ("zoll.toml", "plain = 1"),
        ("pyproject.toml", "[tool.zoll]\npyproject = 1"),
    ])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").arg("check")));
    Ok(())
}

#[test]
fn pyproject_without_zoll_is_passed_over() -> TestResult {
    let repo = Repo::new(&[
        ("zoll.toml", "parent = 1"),
        ("sub/pyproject.toml", "[tool.ruff]\nline-length = 100"),
    ])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("sub").arg("check")));
    Ok(())
}

#[test]
fn config_flag_instead_of_search() -> TestResult {
    let repo = Repo::new(&[("zoll.toml", "found = 1"), ("conf/given.toml", "given = 1")])?;
    repo.settings()?.bind(|| {
        assert_cmd_snapshot!(repo.zoll("").args(["--config", "conf/given.toml", "check"]));
    });
    Ok(())
}

#[test]
fn config_flag_missing_file() -> TestResult {
    let repo = Repo::new(&[])?;
    repo.settings()?.bind(|| {
        assert_cmd_snapshot!(repo.zoll("").args(["check", "--config", "nowhere.toml"]));
    });
    Ok(())
}

// What a broken config says.

#[test]
fn unknown_field() -> TestResult {
    let repo = Repo::new(&[("zoll.toml", "policies = []\nfail_on = \"warn\"\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").arg("check")));
    Ok(())
}

#[test]
fn unknown_field_in_pyproject() -> TestResult {
    let repo = Repo::new(&[("pyproject.toml", "[tool.zoll]\npolicy = [\"policy/\"]\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").arg("check")));
    Ok(())
}

#[test]
fn wrong_type() -> TestResult {
    let repo = Repo::new(&[("zoll.toml", "policies = \"policy/\"\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").arg("check")));
    Ok(())
}

#[test]
fn wrong_value() -> TestResult {
    let repo = Repo::new(&[("zoll.toml", "fail-on = \"error\"\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").arg("check")));
    Ok(())
}

#[test]
fn wrong_type_in_a_table() -> TestResult {
    let repo = Repo::new(&[(
        "zoll.toml",
        "[per-file-ignores]\n\"scripts/**\" = \"bash.curl_pipe\"\n",
    )])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").arg("check")));
    Ok(())
}

#[test]
fn not_toml() -> TestResult {
    let repo = Repo::new(&[("zoll.toml", "policies = [\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(repo.zoll("").arg("check")));
    Ok(())
}

#[test]
fn color_always() -> TestResult {
    let repo = Repo::new(&[("zoll.toml", "x = 1")])?;
    repo.settings()?.bind(|| {
        assert_cmd_snapshot!(repo.zoll("").args(["--color", "always", "check"]));
    });
    Ok(())
}
