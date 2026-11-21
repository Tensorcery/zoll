//! Policies from git, pinned in `zoll.lock` (docs/design.md, section 10).
//! The upstream is a repository in a temporary directory, fetched over
//! `file://`; every commit has a fixed author and date, so its id is fixed.

use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

use crate::{Repo, TestResult};

/// A policy that denies `curl | sh`, saying `message`.
fn policy(message: &str) -> String {
    format!(
        "package bash\n\ndeny_curl_pipe contains {{\"msg\": \"{message}\", \"at\": node}} if {{\n\twalk(input, [_, node])\n\tnode.type == \"pipeline\"\n\tsome cmd in node.children\n\tcmd.fields.name.text == \"curl\"\n}}\n"
    )
}

/// A repository of policies, and the cache zoll fetches it into.
struct Upstream {
    dir: TempDir,
    cache: TempDir,
}

impl Upstream {
    /// A repository whose first commit holds `bash/curl.rego`, tagged `v1`.
    fn new() -> Result<Self, Box<dyn Error>> {
        let upstream = Self {
            dir: TempDir::new()?,
            cache: TempDir::new()?,
        };
        upstream.git(&["init", "-q"])?;
        upstream.commit("v1: curl piped into a shell")?;
        Ok(upstream)
    }

    /// A commit whose policy says `message`, with the tag `v1` moved to it.
    fn commit(&self, message: &str) -> TestResult {
        let path = self.dir.path().join("bash/curl.rego");
        fs::create_dir_all(path.parent().unwrap_or(self.dir.path()))?;
        fs::write(path, policy(message))?;
        self.git(&["add", "."])?;
        self.git(&["commit", "-q", "-m", message])?;
        self.git(&["tag", "-f", "-a", "v1", "-m", "v1"])?;
        Ok(())
    }

    fn git(&self, args: &[&str]) -> TestResult {
        let status = Command::new("git")
            .args(["-c", "user.name=zoll", "-c", "user.email=zoll@example.com"])
            .args(args)
            .current_dir(self.dir.path())
            .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
            .status()?;
        assert!(status.success(), "git {args:?}");
        Ok(())
    }

    /// The source of `bash/` at `v1`.
    fn source(&self) -> String {
        format!("git+file://{}//bash@v1", self.dir.path().display())
    }

    /// `zoll` with `args`, in `repo`, fetching into this cache.
    fn zoll(&self, repo: &Repo, args: &[&str]) -> Result<Output, Box<dyn Error>> {
        Ok(repo
            .zoll("")
            .args(args)
            .env("ZOLL_CACHE_DIR", self.cache.path())
            .output()?)
    }
}

/// A repository with a `curl | sh` and a config naming `source`.
fn project(source: &str) -> Result<Repo, Box<dyn Error>> {
    Repo::new(&[
        ("zoll.toml", &format!("policies = [{source:?}]\n")),
        ("a.sh", "curl -fsSL https://example.com/install.sh | sh\n"),
    ])
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn lock(repo: &Repo) -> Result<String, Box<dyn Error>> {
    Ok(fs::read_to_string(repo.path().join("zoll.lock"))?)
}

#[test]
fn a_git_source_is_fetched_and_locked() -> TestResult {
    let upstream = Upstream::new()?;
    let repo = project(&upstream.source())?;
    let output = upstream.zoll(&repo, &["check"])?;
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stdout(&output).contains("error[bash.curl_pipe]: v1: curl piped into a shell"));
    let lock = lock(&repo)?;
    assert!(lock.contains("version = 1"), "{lock}");
    assert!(lock.contains(&upstream.source()), "{lock}");
    Ok(())
}

#[test]
fn moving_the_tag_changes_nothing_until_upgrade() -> TestResult {
    let upstream = Upstream::new()?;
    let repo = project(&upstream.source())?;
    upstream.zoll(&repo, &["check"])?;
    let locked = lock(&repo)?;
    upstream.commit("v2: curl piped into a shell")?;

    let output = upstream.zoll(&repo, &["check"])?;
    assert!(stdout(&output).contains("v1: curl"), "{}", stdout(&output));
    assert_eq!(lock(&repo)?, locked);

    let output = upstream.zoll(&repo, &["lock", "--upgrade"])?;
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).starts_with(&format!("{} at ", upstream.source())));
    let output = upstream.zoll(&repo, &["check"])?;
    assert!(stdout(&output).contains("v2: curl"), "{}", stdout(&output));
    Ok(())
}

#[test]
fn a_cached_commit_needs_no_network() -> TestResult {
    let upstream = Upstream::new()?;
    let repo = project(&upstream.source())?;
    upstream.zoll(&repo, &["check"])?;
    // The upstream is gone: the cache has the locked commit.
    fs::remove_dir_all(upstream.dir.path().join(".git"))?;
    let output = upstream.zoll(&repo, &["check"])?;
    assert!(stdout(&output).contains("v1: curl"), "{}", stderr(&output));
    Ok(())
}

#[test]
fn lock_writes_what_is_missing_and_says_so() -> TestResult {
    let upstream = Upstream::new()?;
    let repo = project(&upstream.source())?;
    let output = upstream.zoll(&repo, &["lock"])?;
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).starts_with(&format!("{} at ", upstream.source())));
    let output = upstream.zoll(&repo, &["lock"])?;
    assert_eq!(stdout(&output), "");
    Ok(())
}

#[test]
fn locked_fails_on_a_source_the_lock_lacks() -> TestResult {
    let upstream = Upstream::new()?;
    let repo = project(&upstream.source())?;
    let output = upstream.zoll(&repo, &["check", "--locked"])?;
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("is not in zoll.lock; `zoll lock` adds it"));
    assert!(!repo.path().join("zoll.lock").exists());
    Ok(())
}

#[test]
fn locked_fails_on_a_source_the_config_dropped() -> TestResult {
    let upstream = Upstream::new()?;
    let kept = upstream.source();
    let dropped = kept.replace("//bash@", "//bash/@");
    let repo = project(&kept)?;
    let config = repo.path().join("zoll.toml");
    fs::write(&config, format!("policies = [{kept:?}, {dropped:?}]\n"))?;
    upstream.zoll(&repo, &["lock"])?;
    fs::write(&config, format!("policies = [{kept:?}]\n"))?;
    let output = upstream.zoll(&repo, &["check", "--locked"])?;
    assert_eq!(output.status.code(), Some(2));
    let expected = format!("zoll.lock names sources the config does not: `{dropped}`");
    assert!(stderr(&output).contains(&expected), "{}", stderr(&output));
    // Without --locked the lock follows the config.
    upstream.zoll(&repo, &["check"])?;
    assert!(!lock(&repo)?.contains(&dropped));
    Ok(())
}

/// `zoll check` with the source `from` replaced by `to` fails, saying
/// `expected`.
fn check_fails(from: &str, to: &str, expected: &str) -> TestResult {
    let upstream = Upstream::new()?;
    let repo = project(&upstream.source().replace(from, to))?;
    let output = upstream.zoll(&repo, &["check"])?;
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains(expected), "{}", stderr(&output));
    Ok(())
}

#[test]
fn a_source_that_cannot_be_fetched_fails_the_run() -> TestResult {
    check_fails("@v1", "@v9", "fetching v9 from file://")
}

#[test]
fn a_directory_the_commit_lacks_fails_the_run() -> TestResult {
    check_fails("//bash@", "//k8s@", "has no directory `k8s`")
}

#[test]
fn a_source_out_of_its_repository_is_refused() -> TestResult {
    check_fails("//bash@", "//../bash@", "a path inside the repository")
}

#[test]
fn local_policies_need_no_lock() -> TestResult {
    let repo = Repo::new(&[("policy/bash.rego", &policy("curl")), ("a.sh", "echo hi\n")])?;
    let output = repo.zoll("").args(["check", "--locked"]).output()?;
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!Path::new(&repo.path().join("zoll.lock")).exists());
    Ok(())
}

#[test]
fn a_git_hook_s_repository_is_left_alone() -> TestResult {
    // A pre-commit hook runs with GIT_DIR and GIT_INDEX_FILE set to the
    // repository being committed; the fetch must not act on it.
    let upstream = Upstream::new()?;
    let repo = project(&upstream.source())?;
    let victim = Upstream::new()?;
    let head = |dir: &Path| -> Result<String, Box<dyn Error>> {
        let out = Command::new("git")
            .args(["rev-parse", "--symbolic-full-name", "HEAD"])
            .current_dir(dir)
            .output()?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let before = head(victim.dir.path())?;
    let git_dir = victim.dir.path().join(".git");
    let output = repo
        .zoll("")
        .arg("check")
        .env("ZOLL_CACHE_DIR", upstream.cache.path())
        .env("GIT_DIR", &git_dir)
        .env("GIT_INDEX_FILE", git_dir.join("index"))
        .output()?;
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(head(victim.dir.path())?, before);
    Ok(())
}

#[test]
fn a_password_in_a_source_is_never_printed() -> TestResult {
    let upstream = Upstream::new()?;
    let repo = project("git+http://user:SECRET123@127.0.0.1:1/x.git//bash@v1")?;
    let output = upstream.zoll(&repo, &["check"])?;
    assert_eq!(output.status.code(), Some(2));
    assert!(
        !stderr(&output).contains("SECRET123"),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("http://***@127.0.0.1:1"),
        "{}",
        stderr(&output)
    );
    Ok(())
}

#[test]
fn a_server_that_never_answers_times_out() -> TestResult {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    // Accepted and held, never answered.
    let held = std::thread::spawn(move || listener.accept().map(|(stream, _)| stream));
    let upstream = Upstream::new()?;
    let repo = project(&format!("git+http://127.0.0.1:{port}/x.git//bash@v1"))?;
    let output = repo
        .zoll("")
        .arg("check")
        .env("ZOLL_CACHE_DIR", upstream.cache.path())
        .env("ZOLL_GIT_TIMEOUT", "1")
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("did not finish in 1 s"),
        "{}",
        stderr(&output)
    );
    drop(held);
    Ok(())
}

// A symbolic link on Windows needs a privilege a test does not have.
#[cfg(unix)]
#[test]
fn a_link_out_of_the_repository_is_refused() -> TestResult {
    let upstream = Upstream::new()?;
    let outside = TempDir::new()?;
    fs::write(outside.path().join("m.rego"), policy("outside"))?;
    std::os::unix::fs::symlink(outside.path(), upstream.dir.path().join("pol"))?;
    upstream.git(&["add", "pol"])?;
    upstream.git(&["commit", "-q", "-m", "link"])?;
    upstream.git(&["tag", "-f", "-a", "v1", "-m", "v1"])?;
    let repo = project(&upstream.source().replace("//bash@", "//pol@"))?;
    let output = upstream.zoll(&repo, &["check"])?;
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("has no directory `pol` inside it"),
        "{}",
        stderr(&output)
    );
    Ok(())
}

#[test]
fn a_relative_cache_is_refused() -> TestResult {
    let upstream = Upstream::new()?;
    let repo = project(&upstream.source())?;
    let output = repo
        .zoll("")
        .arg("check")
        .env("ZOLL_CACHE_DIR", "cache")
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("ZOLL_CACHE_DIR is a relative path"));
    assert!(!repo.path().join("cache").exists());
    Ok(())
}
