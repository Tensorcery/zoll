//! Which files are checked, and the language each one is taken for:
//! `zoll check --show-files` lists them.

use crate::{Repo, TestResult};
use insta_cmd::assert_cmd_snapshot;

fn show_files(repo: &Repo) -> std::process::Command {
    let mut command = repo.zoll("");
    command.args(["check", "--show-files"]);
    command
}

#[test]
fn by_name_and_extension() -> TestResult {
    let repo = Repo::new(&[
        ("a.yml", ""),
        ("b.yaml", ""),
        ("c.json", ""),
        ("pyproject.toml", "[project]\nname = \"x\"\n"),
        ("Dockerfile", ""),
        ("Dockerfile.dev", ""),
        ("Makefile", ""),
        ("Jenkinsfile", ""),
        ("main.tf", ""),
        ("run.sh", ""),
        ("src/app.py", ""),
        ("src/lib.rs", ""),
        ("web/index.tsx", ""),
        ("notes.txt", ""),
        ("LICENSE", ""),
    ])?;
    assert_cmd_snapshot!(show_files(&repo));
    Ok(())
}

#[test]
fn languages_of_the_wider_set() -> TestResult {
    let repo = Repo::new(&[
        ("chart/templates/_helpers.tpl", ""),
        ("default.nix", ""),
        ("BUILD.bazel", ""),
        ("defs.bzl", ""),
        ("CMakeLists.txt", ""),
        ("api.proto", ""),
        ("schema.graphql", ""),
        ("setup.cfg", ""),
        ("nginx.conf", ""),
        ("main.jsonnet", ""),
        ("justfile", ""),
        ("init.lua", ""),
        ("tidy.pl", ""),
        ("style.scss", ""),
        ("App.svelte", ""),
        ("Main.scala", ""),
        ("build.sbt", ""),
        ("main.dart", ""),
        ("mix.exs", ""),
        ("Main.hs", ""),
        ("build.zig", ""),
    ])?;
    assert_cmd_snapshot!(show_files(&repo));
    Ok(())
}

#[test]
fn by_shebang() -> TestResult {
    let repo = Repo::new(&[
        ("bin/deploy", "#!/bin/sh\necho hi\n"),
        ("bin/report", "#!/usr/bin/env python3.12\nprint()\n"),
        ("bin/columns", "#!/usr/bin/env -S awk -f\n{ print $1 }\n"),
        ("bin/serve", "#!/usr/bin/env node\n"),
        ("bin/tidy", "#!/usr/bin/perl -w\n"),
        ("bin/hook", "#!/usr/bin/env lua\n"),
        ("bin/unknown", "#!/usr/bin/tclsh\n"),
        ("bin/plain", "no shebang\n"),
    ])?;
    assert_cmd_snapshot!(show_files(&repo));
    Ok(())
}

#[test]
fn config_languages_first() -> TestResult {
    let repo = Repo::new(&[
        (
            "zoll.toml",
            "[languages]\n\"Jenkinsfile.*\" = \"groovy\"\n\"ci/*.conf\" = \"yaml\"\n\"*.txt\" = \"bash\"\n",
        ),
        ("Jenkinsfile.release", ""),
        ("ci/build.conf", ""),
        ("other/build.conf", ""),
        ("notes.txt", ""),
    ])?;
    assert_cmd_snapshot!(show_files(&repo));
    Ok(())
}

#[test]
fn unknown_language_in_config() -> TestResult {
    let repo = Repo::new(&[("zoll.toml", "[languages]\n\"*.conf\" = \"yml\"\n")])?;
    repo.settings()?
        .bind(|| assert_cmd_snapshot!(show_files(&repo)));
    Ok(())
}

#[test]
fn gitignore_honoured_outside_git() -> TestResult {
    let repo = Repo::new(&[
        (".gitignore", "build/\n*.generated.yml\n"),
        ("build/out.yml", ""),
        ("a.generated.yml", ""),
        ("a.yml", ""),
    ])?;
    assert_cmd_snapshot!(show_files(&repo));
    Ok(())
}

#[test]
fn hidden_walked_but_not_git() -> TestResult {
    let repo = Repo::new(&[
        (".github/workflows/ci.yml", ""),
        (".pre-commit-config.yaml", ""),
        (".git/config.yml", ""),
    ])?;
    assert_cmd_snapshot!(show_files(&repo));
    Ok(())
}

#[test]
fn include_and_exclude() -> TestResult {
    let repo = Repo::new(&[
        (
            "zoll.toml",
            "include = [\"**/*.yml\", \"**/*.json\"]\nexclude = [\"vendor/**\", \"**/*.lock.json\"]\n",
        ),
        ("a.yml", ""),
        ("b.json", ""),
        ("c.lock.json", ""),
        ("d.toml", ""),
        ("vendor/e.yml", ""),
        ("src/vendor/f.yml", ""),
    ])?;
    assert_cmd_snapshot!(show_files(&repo));
    Ok(())
}

#[test]
fn globs_are_relative_to_the_config() -> TestResult {
    let repo = Repo::new(&[
        ("zoll.toml", "exclude = [\"sub/skip/**\"]\n"),
        ("sub/a.yml", ""),
        ("sub/skip/b.yml", ""),
    ])?;
    assert_cmd_snapshot!(repo.zoll("sub").args(["check", "--show-files"]));
    Ok(())
}

#[test]
fn paths_given() -> TestResult {
    let repo = Repo::new(&[
        ("zoll.toml", "exclude = [\"vendor/**\"]\n"),
        ("a.yml", ""),
        ("src/b.yml", ""),
        ("src/c.json", ""),
        ("vendor/d.yml", ""),
    ])?;
    // A file named on the command line is checked even when excluded, as
    // ruff does.
    assert_cmd_snapshot!(
        repo.zoll("")
            .args(["check", "--show-files", "src", "vendor/d.yml"])
    );
    Ok(())
}

#[test]
fn force_exclude_leaves_out_a_named_file() -> TestResult {
    let repo = Repo::new(&[
        ("zoll.toml", "exclude = [\"vendor/**\"]\n"),
        ("a.yml", ""),
        ("vendor/d.yml", ""),
    ])?;
    // What a pre-commit hook does: name each staged file.
    assert_cmd_snapshot!(repo.zoll("").args([
        "check",
        "--show-files",
        "--force-exclude",
        "a.yml",
        "vendor/d.yml"
    ]));
    Ok(())
}

#[test]
fn path_that_does_not_exist() -> TestResult {
    let repo = Repo::new(&[])?;
    repo.settings()?.bind(|| {
        assert_cmd_snapshot!(repo.zoll("").args(["check", "--show-files", "nowhere"]));
    });
    Ok(())
}

#[test]
fn an_extension_outranks_the_name_before_the_dot() -> TestResult {
    let repo = Repo::new(&[
        ("BUILD.md", ""),
        ("Pipfile.lock", "{}"),
        ("Gemfile.lock", ""),
        ("Dockerfile.dev", ""),
        ("Makefile.release", ""),
    ])?;
    assert_cmd_snapshot!(show_files(&repo));
    Ok(())
}

#[test]
fn a_shebang_is_read_up_to_its_line_end() -> TestResult {
    // Byte 128 falls inside a two-byte letter of the second line.
    let cut = format!("#!/bin/sh\n#  {}\n", "я".repeat(60));
    let repo = Repo::new(&[("bin/cut", &cut)])?;
    assert_cmd_snapshot!(show_files(&repo));
    Ok(())
}

#[test]
fn languages_apply_in_the_order_written() -> TestResult {
    let repo = Repo::new(&[
        (
            "zoll.toml",
            "[languages]\n\"ci/*.conf\" = \"yaml\"\n\"*.conf\" = \"json\"\n",
        ),
        ("ci/build.conf", ""),
        ("other.conf", ""),
    ])?;
    assert_cmd_snapshot!(show_files(&repo));
    Ok(())
}

#[test]
fn only_gitignore_hides_files() -> TestResult {
    let repo = Repo::new(&[(".ignore", "ci/\n"), ("ci/a.sh", "")])?;
    assert_cmd_snapshot!(show_files(&repo));
    Ok(())
}

#[test]
fn a_parent_path_is_the_project() -> TestResult {
    let repo = Repo::new(&[
        (
            "zoll.toml",
            "exclude = [\"vendor/**\"]\n[languages]\n\"ci/*.conf\" = \"yaml\"\n",
        ),
        ("ci/x.conf", ""),
        ("sub/s.yaml", ""),
        ("vendor/v.yaml", ""),
    ])?;
    repo.settings()?.bind(|| {
        assert_cmd_snapshot!(repo.zoll("sub").args(["check", "--show-files", ".."]));
    });
    Ok(())
}
