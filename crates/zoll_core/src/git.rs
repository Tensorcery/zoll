//! Policies from git: `git+<url>//<dir>@<ref>` fetched with the system `git`
//! into a cache, pinned to a commit in `zoll.lock` (docs/design.md,
//! section 10).

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// The lock file, beside the config.
pub const LOCK: &str = "zoll.lock";

/// The version of the lock's format this zoll writes and reads.
const VERSION: u32 = 1;

/// A git source of `policies`.
#[derive(Debug)]
struct Source<'a> {
    /// As the config writes it.
    text: &'a str,
    url: &'a str,
    /// The directory in the repository; empty for its root.
    dir: &'a str,
    /// A tag, a branch or a commit.
    reference: &'a str,
}

impl<'a> Source<'a> {
    fn parse(text: &'a str) -> Result<Self, Error> {
        let bad = |why: &str| {
            Error::Other(format!(
                "`{}`: {why}; a git source is `git+<url>//<directory>@<tag-or-commit>`",
                redact(text)
            ))
        };
        let rest = text.strip_prefix("git+").ok_or_else(|| bad("no `git+`"))?;
        // The `//` after the URL's own `scheme://`.
        let after_scheme = rest.find("://").map_or(0, |i| i + 3);
        let split = rest[after_scheme..]
            .find("//")
            .map(|i| i + after_scheme)
            .ok_or_else(|| bad("no `//` before the directory"))?;
        let url = &rest[..split];
        let (dir, reference) = rest[split + 2..]
            .rsplit_once('@')
            .ok_or_else(|| bad("no `@` before the tag or commit"))?;
        let dir = dir.trim_end_matches('/');
        // Nothing reaches out of the checkout, and nothing reads as an option
        // to git.
        if !Path::new(dir)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        {
            return Err(bad("the directory is a path inside the repository"));
        }
        if url.is_empty() || url.starts_with('-') {
            return Err(bad("no URL"));
        }
        if reference.is_empty() || reference.starts_with('-') {
            return Err(bad("no tag or commit"));
        }
        Ok(Self {
            text,
            url,
            dir,
            reference,
        })
    }
}

/// What a run does with the lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `zoll check`: the locked commits; a source the lock lacks is resolved
    /// and written, as `uv run` does.
    Check,
    /// `zoll check --locked`: the locked commits, and a lock that does not
    /// match the config fails.
    Locked,
    /// `zoll lock`: the sources the lock lacks are resolved.
    Lock,
    /// `zoll lock --upgrade`: every ref is resolved again.
    Upgrade,
}

/// The git sources of a run, fetched.
#[derive(Debug, Default)]
pub struct Resolved {
    /// The directory each source is read from, by its text in the config.
    pub dirs: HashMap<String, PathBuf>,
    /// The sources whose locked commit changed, with the new commit.
    pub changed: Vec<(String, String)>,
}

#[derive(Debug, Deserialize, Serialize)]
struct LockFile {
    version: u32,
    /// The commit of each source, by its text in the config.
    policies: BTreeMap<String, String>,
}

/// The git sources among `policies`, each at its commit: locked, or
/// resolved and written to `root/zoll.lock` as `mode` says. A config with no
/// git source has no lock.
///
/// # Errors
///
/// A source that does not parse or cannot be fetched, a lock that cannot be
/// read or written, or one that does not match the config under
/// [`Mode::Locked`].
pub fn resolve(root: &Path, policies: &[String], mode: Mode) -> Result<Resolved, Error> {
    let sources = policies
        .iter()
        .filter(|p| p.starts_with("git+"))
        .map(|p| Source::parse(p))
        .collect::<Result<Vec<_>, _>>()?;
    let mut resolved = Resolved::default();
    let path = root.join(LOCK);
    if sources.is_empty() && !path.exists() {
        return Ok(resolved);
    }
    let old = read_lock(&path)?;
    let cache = cache_dir()?;
    let mut new = BTreeMap::new();
    for source in &sources {
        let locked = old.get(source.text).filter(|_| mode != Mode::Upgrade);
        let (commit, tree) = match locked {
            Some(commit) => (commit.clone(), at_commit(&cache, source, commit)?),
            None if mode == Mode::Locked => {
                return Err(Error::Other(format!(
                    "`{}` is not in {LOCK}; `zoll lock` adds it",
                    redact(source.text)
                )));
            }
            None => fetch(&cache, source.url, source.reference)?,
        };
        let dir = inside(&tree, source.dir).ok_or_else(|| {
            Error::Other(format!(
                "`{}`: commit {} has no directory `{}` inside it",
                redact(source.text),
                short(&commit),
                source.dir
            ))
        })?;
        if old.get(source.text) != Some(&commit) {
            resolved
                .changed
                .push((source.text.to_owned(), commit.clone()));
        }
        resolved.dirs.insert(source.text.to_owned(), dir);
        new.insert(source.text.to_owned(), commit);
    }
    if new != old {
        if mode == Mode::Locked {
            let stale: Vec<_> = old.keys().filter(|k| !new.contains_key(*k)).collect();
            return Err(Error::Other(format!(
                "{LOCK} names sources the config does not: {}; `zoll lock` drops them",
                stale
                    .iter()
                    .map(|s| format!("`{}`", redact(s)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        write_lock(&path, new)?;
    }
    Ok(resolved)
}

/// The first twelve characters of a commit, as a person reads it.
#[must_use]
pub fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

fn read_lock(path: &Path) -> Result<BTreeMap<String, String>, Error> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(Error::Io(path.to_owned(), e)),
    };
    let bad = |why: String| Error::Other(format!("{}: {why}", path.display()));
    let lock: LockFile = toml::from_str(&text).map_err(|e| bad(e.message().to_owned()))?;
    if lock.version != VERSION {
        return Err(bad(format!(
            "version {} is not {VERSION}, the one this zoll reads",
            lock.version
        )));
    }
    // A commit names a directory of the cache: hex, nothing else.
    if let Some((source, commit)) = lock.policies.iter().find(|(_, c)| !is_commit(c)) {
        return Err(bad(format!("`{commit}` of `{source}` is not a commit")));
    }
    Ok(lock.policies)
}

fn write_lock(path: &Path, policies: BTreeMap<String, String>) -> Result<(), Error> {
    let lock = LockFile {
        version: VERSION,
        policies,
    };
    let body = toml::to_string(&lock).map_err(|e| Error::Other(e.to_string()))?;
    let text = format!(
        "# The commit each git source of `policies` resolved to. Written by zoll;\n\
         # `zoll lock --upgrade` moves them.\n{body}"
    );
    std::fs::write(path, text).map_err(|e| Error::Io(path.to_owned(), e))
}

/// A full commit id: SHA-1 or SHA-256, in hex.
fn is_commit(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `$ZOLL_CACHE_DIR`, else `zoll/policies` in the user's cache directory.
/// A relative directory is passed over, as the XDG spec says of its own: a
/// cache inside the project would be walked and committed with it.
fn cache_dir() -> Result<PathBuf, Error> {
    let var = |name| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    if let Some(dir) = var("ZOLL_CACHE_DIR") {
        return Ok(dir);
    }
    if std::env::var_os("ZOLL_CACHE_DIR").is_some_and(|v| !v.is_empty()) {
        return Err(Error::Other(
            "ZOLL_CACHE_DIR is a relative path; name the cache by an absolute one".to_owned(),
        ));
    }
    let cache = var("XDG_CACHE_HOME")
        .or_else(|| var("HOME").map(|home| home.join(".cache")))
        .or_else(|| var("LOCALAPPDATA"))
        .ok_or_else(|| Error::Other("no cache directory: set ZOLL_CACHE_DIR or HOME".to_owned()))?;
    Ok(cache.join("zoll").join("policies"))
}

/// The tree of a locked commit: from the cache, else fetched by its id.
fn at_commit(cache: &Path, source: &Source<'_>, commit: &str) -> Result<PathBuf, Error> {
    let tree = cache.join(commit);
    if tree.is_dir() {
        return Ok(tree);
    }
    let (fetched, tree) = fetch(cache, source.url, commit)?;
    if fetched != commit {
        return Err(Error::Other(format!(
            "`{}`: fetching {commit} gave {fetched}",
            redact(source.text)
        )));
    }
    Ok(tree)
}

/// `dir` of the checkout `tree`, when it is a directory and stays inside
/// the tree with its links followed: a link in the repository may point
/// anywhere on the disk.
fn inside(tree: &Path, dir: &str) -> Option<PathBuf> {
    let tree = tree.canonicalize().ok()?;
    let dir = tree.join(dir).canonicalize().ok()?;
    (dir.starts_with(&tree) && dir.is_dir()).then_some(dir)
}

/// `user:password@` of a URL, as `***@`: messages and logs never carry it.
#[must_use]
pub fn redact(text: &str) -> String {
    let Some(scheme) = text.find("://").map(|i| i + 3) else {
        return text.to_owned();
    };
    let host_end = text[scheme..].find('/').map_or(text.len(), |i| scheme + i);
    match text[scheme..host_end].rfind('@') {
        Some(at) => format!("{}***{}", &text[..scheme], &text[scheme + at..]),
        None => text.to_owned(),
    }
}

/// How long a `.fetch-*` directory may sit in the cache before a run takes
/// it for one a killed run left: far past any fetch's [`timeout`].
const STALE: Duration = Duration::from_hours(24);

/// Removes the work directories killed runs left in the cache.
fn sweep(cache: &Path) {
    let Ok(entries) = std::fs::read_dir(cache) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > STALE);
        if stale && entry.file_name().to_string_lossy().starts_with(".fetch-") {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Fetches the one commit `reference` names, shallow, and keeps its tree in
/// the cache under the commit's id: the commit and that directory.
fn fetch(cache: &Path, url: &str, reference: &str) -> Result<(String, PathBuf), Error> {
    std::fs::create_dir_all(cache).map_err(|e| Error::Io(cache.to_owned(), e))?;
    sweep(cache);
    let work = tempfile::Builder::new()
        .prefix(".fetch-")
        .tempdir_in(cache)
        .map_err(|e| Error::Io(cache.to_owned(), e))?;
    let dir = work.path();
    git(dir, &["init", "-q"])?;
    git(
        dir,
        &[
            "fetch",
            "-q",
            "--depth",
            "1",
            "--end-of-options",
            url,
            reference,
        ],
    )
    .map_err(|e| Error::Other(format!("fetching {reference} from {}: {e}", redact(url))))?;
    let commit = git(dir, &["rev-parse", "--verify", "FETCH_HEAD^{commit}"])?;
    let commit = commit.trim().to_owned();
    if !is_commit(&commit) {
        return Err(Error::Other(format!("git named the commit `{commit}`")));
    }
    let tree = cache.join(&commit);
    if !tree.is_dir() {
        git(
            dir,
            &[
                "-c",
                "advice.detachedHead=false",
                "checkout",
                "-q",
                "FETCH_HEAD",
            ],
        )?;
        let repository = dir.join(".git");
        std::fs::remove_dir_all(&repository).map_err(|e| Error::Io(repository, e))?;
        let kept = work.keep();
        // Another run may have kept the same commit meanwhile: theirs stays.
        if std::fs::rename(&kept, &tree).is_err() {
            let _ = std::fs::remove_dir_all(&kept);
        }
    }
    Ok((commit, tree))
}

/// The variables that point git at a repository: a git hook runs with them
/// set to the repository being committed, which git would then act on
/// instead of `dir` (`git rev-parse --local-env-vars`, less the config ones,
/// which carry the user's own settings).
const REPOSITORY_VARS: [&str; 13] = [
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_DIR",
    "GIT_GRAFT_FILE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_INTERNAL_SUPER_PREFIX",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_REPLACE_REF_BASE",
    "GIT_SHALLOW_FILE",
    "GIT_WORK_TREE",
];

/// How long one git command may run: `$ZOLL_GIT_TIMEOUT` seconds, else ten
/// minutes. A server that accepts and never answers would hang the run.
fn timeout() -> Duration {
    let seconds = std::env::var("ZOLL_GIT_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600);
    Duration::from_secs(seconds)
}

/// Runs `git` in `dir`; its standard output, or its error.
fn git(dir: &Path, args: &[&str]) -> Result<String, Error> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(dir)
        // Credentials come from git's helpers and the SSH agent; a prompt
        // nobody answers would hang a CI job.
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for var in REPOSITORY_VARS {
        command.env_remove(var);
    }
    let mut child = command
        .spawn()
        .map_err(|e| Error::Other(format!("git: {e}; zoll fetches policies with git")))?;
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = read(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = read(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + timeout();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                // Git for Windows' `git` starts the real one, which starts
                // `git-remote-http`: killing the first alone leaves them
                // running, holding the pipes it inherited.
                #[cfg(windows)]
                let _ = Command::new("taskkill")
                    .args(["/T", "/F", "/PID", &child.id().to_string()])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Other(format!(
                    "git {} did not finish in {} s (ZOLL_GIT_TIMEOUT)",
                    args.first().unwrap_or(&""),
                    timeout().as_secs()
                )));
            }
            Err(e) => return Err(Error::Other(format!("git: {e}"))),
        }
    };
    let output = std::process::Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Other(stderr.trim().to_owned()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_loses_its_password() {
        assert_eq!(
            redact("git+https://x-access-token:T0K@github.com/o/r//p@v1"),
            "git+https://***@github.com/o/r//p@v1"
        );
        assert_eq!(
            redact("git+ssh://git@host/r//p@v1"),
            "git+ssh://***@host/r//p@v1"
        );
        assert_eq!(
            redact("git+https://host/r//p@v1"),
            "git+https://host/r//p@v1"
        );
    }

    #[test]
    fn a_killed_run_s_work_directory_is_swept() -> Result<(), Box<dyn std::error::Error>> {
        let cache = tempfile::TempDir::new()?;
        let old = cache.path().join(".fetch-old");
        let new = cache.path().join(".fetch-new");
        std::fs::create_dir(&old)?;
        std::fs::create_dir(&new)?;
        let long_ago = std::time::SystemTime::now() - STALE - Duration::from_secs(60);
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        // Windows opens a directory only with FILE_FLAG_BACKUP_SEMANTICS,
        // and sets its time only with FILE_WRITE_ATTRIBUTES.
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.access_mode(0x100).custom_flags(0x0200_0000);
        }
        options.open(&old)?.set_modified(long_ago)?;
        sweep(cache.path());
        assert!(!old.exists());
        assert!(new.exists(), "a fetch under way stays");
        Ok(())
    }

    #[test]
    fn a_source_splits_after_the_url() {
        let source = Source::parse("git+ssh://git@github.com/my-org/policies//k8s/base@v1.0.0");
        let source = source.map(|s| (s.url, s.dir, s.reference)).ok();
        assert_eq!(
            source,
            Some(("ssh://git@github.com/my-org/policies", "k8s/base", "v1.0.0"))
        );
    }

    #[test]
    fn a_source_stays_in_its_repository() {
        for text in [
            "git+https://x/r//../etc@v1",
            "git+https://x/r///etc@v1",
            "git+https://x/r//p@-c",
            "git+https://x/r/p@v1",
            "git+https://x/r//p",
        ] {
            assert!(Source::parse(text).is_err(), "{text}");
        }
    }
}
