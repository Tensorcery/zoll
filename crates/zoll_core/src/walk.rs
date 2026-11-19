//! The files a run checks: walked in parallel, `.gitignore` honoured, the
//! config's `include` and `exclude` applied.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use globset::{GlobSet, GlobSetBuilder};
use ignore::{WalkBuilder, WalkState};

use crate::config::Config;
use crate::error::Error;
use crate::language::{Languages, glob_of};

/// Directories of version control, never walked although hidden ones are.
const VCS: [&str; 4] = [".git", ".hg", ".jj", ".svn"];

/// A file to check.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct File {
    /// Where it is, absolute.
    pub path: PathBuf,
    /// Its path from the config's directory, `/` between the parts; the
    /// absolute path when it is outside that directory.
    pub relative: String,
    /// Its language.
    pub language: &'static str,
}

/// The files under `paths` that have a language, in path order.
///
/// A file named in `paths` itself is checked even when `include` or
/// `exclude` would leave it out, as ruff does.
///
/// # Errors
///
/// A path that does not exist, a directory that cannot be read, a glob of
/// the config that does not compile.
pub fn files(
    paths: &[PathBuf],
    root: &Path,
    config: &Config,
    languages: &Languages,
) -> Result<Vec<File>, Error> {
    for path in paths {
        std::fs::metadata(path).map_err(|e| Error::Io(path.clone(), e))?;
    }
    let include = globs(&config.include)?;
    let exclude = globs(&config.exclude)?;
    let Some((first, rest)) = paths.split_first() else {
        return Ok(Vec::new());
    };
    let mut walk = WalkBuilder::new(first);
    for path in rest {
        walk.add(path);
    }
    walk.hidden(false)
        // `.gitignore` is honoured in a directory that is not a repository
        // yet, too.
        .require_git(false)
        .filter_entry(|entry| !VCS.iter().any(|vcs| entry.file_name() == *vcs));

    let found = Mutex::new(Vec::new());
    let failed = Mutex::new(None);
    walk.build_parallel().run(|| {
        Box::new(|entry| {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    if let Ok(mut failed) = failed.lock() {
                        failed.get_or_insert(error.to_string());
                    }
                    return WalkState::Quit;
                }
            };
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                return WalkState::Continue;
            }
            let path = entry.path();
            let relative = relative(path, root);
            let named = entry.depth() == 0;
            // ponytail: `exclude` is matched against files, so an excluded
            // directory is still walked; prune it when a big one shows.
            let wanted = named || (include.is_match(&relative) && !exclude.is_match(&relative));
            if wanted && let Some(language) = languages.of(Path::new(&relative), path) {
                let file = File {
                    path: path.to_owned(),
                    relative,
                    language,
                };
                if let Ok(mut found) = found.lock() {
                    found.push(file);
                }
            }
            WalkState::Continue
        })
    });

    if let Some(error) = failed.into_inner().ok().flatten() {
        return Err(Error::Other(error));
    }
    let mut found = found
        .into_inner()
        .map_err(|_| Error::Other("a walking thread panicked".to_owned()))?;
    found.sort();
    Ok(found)
}

/// The path from `root`, `/` between the parts; the path itself when it is
/// not under `root`.
#[must_use]
pub fn relative(path: &Path, root: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let parts: Vec<_> = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect();
    if relative.is_absolute() {
        relative.display().to_string()
    } else {
        parts.join("/")
    }
}

fn globs(globs: &[String]) -> Result<GlobSet, Error> {
    let mut set = GlobSetBuilder::new();
    for glob in globs {
        set.add(glob_of(glob)?);
    }
    set.build().map_err(|e| Error::Other(e.to_string()))
}
