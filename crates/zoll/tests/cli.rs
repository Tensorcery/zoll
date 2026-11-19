//! The command as a user runs it: the binary, its arguments, its output.

use insta_cmd::{assert_cmd_snapshot, get_cargo_bin};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

fn zoll() -> Command {
    Command::new(get_cargo_bin("zoll"))
}

/// A directory with these files in it, standing in for a repository.
struct Repo(TempDir);

impl Repo {
    fn new(files: &[(&str, &str)]) -> Result<Self, Box<dyn Error>> {
        let dir = TempDir::new()?;
        for (path, text) in files {
            let path = dir.path().join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, text)?;
        }
        Ok(Self(dir))
    }

    /// `zoll` started in `subdir` of the repository.
    fn zoll(&self, subdir: &str) -> Command {
        let mut command = zoll();
        command.current_dir(self.0.path().join(subdir));
        command
    }

    /// Snapshot settings that write the repository's path as `[REPO]`.
    fn settings(&self) -> Result<insta::Settings, Box<dyn Error>> {
        let path = self.0.path().canonicalize()?;
        let mut settings = insta::Settings::clone_current();
        settings.add_filter(&escape(&path), "[REPO]");
        Ok(settings)
    }
}

fn escape(path: &Path) -> String {
    path.display().to_string().replace('.', r"\.")
}

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

// The commands and their flags.

#[test]
fn help() {
    assert_cmd_snapshot!(zoll().arg("--help"));
}

#[test]
fn check_help() {
    assert_cmd_snapshot!(zoll().args(["check", "--help"]));
}

#[test]
fn version() {
    assert_cmd_snapshot!(zoll().arg("--version"));
}

#[test]
fn no_command() {
    assert_cmd_snapshot!(zoll());
}

#[test]
fn unknown_format() {
    assert_cmd_snapshot!(zoll().args(["check", "--format", "xml"]));
}

#[test]
fn inspect_not_built() -> TestResult {
    let repo = Repo::new(&[])?;
    assert_cmd_snapshot!(repo.zoll("").args(["inspect", "a.yml"]));
    Ok(())
}

#[test]
fn lock_not_built() -> TestResult {
    let repo = Repo::new(&[])?;
    assert_cmd_snapshot!(repo.zoll("").args(["lock", "--upgrade"]));
    Ok(())
}

#[test]
fn languages_not_built() -> TestResult {
    let repo = Repo::new(&[])?;
    assert_cmd_snapshot!(repo.zoll("").arg("languages"));
    Ok(())
}

// A valid config, or none.

#[test]
fn check_without_config() -> TestResult {
    let repo = Repo::new(&[])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
    Ok(())
}

#[test]
fn check_every_field() -> TestResult {
    let repo = Repo::new(&[("zoll.toml", VALID)])?;
    assert_cmd_snapshot!(
        repo.zoll("")
            .args(["check", "src", "--format", "json", "--locked"])
    );
    Ok(())
}

#[test]
fn check_pyproject_every_field() -> TestResult {
    let pyproject = format!(
        "[project]\nname = \"x\"\n\n[tool.zoll]\n{}",
        VALID.replace("\n[", "\n[tool.zoll.")
    );
    let repo = Repo::new(&[("pyproject.toml", &pyproject)])?;
    assert_cmd_snapshot!(repo.zoll("").arg("check"));
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
