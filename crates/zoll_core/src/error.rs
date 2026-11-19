//! What can go wrong, as values the command shows.

use std::fmt;
use std::io;
use std::ops::Range;
use std::path::PathBuf;

/// Why zoll could not go on.
#[derive(Debug)]
pub enum Error {
    /// A file zoll could not read.
    Io(PathBuf, io::Error),
    /// A file that does not parse, with the place it breaks at.
    Source(SourceError),
    /// Anything else, in words.
    Other(String),
}

/// A file that does not parse: what is wrong, and where.
#[derive(Debug)]
pub struct SourceError {
    /// The file.
    pub path: PathBuf,
    /// Its text, for the command to show the line.
    pub text: String,
    /// What is wrong; may go on with `, expected ...`.
    pub message: String,
    /// Where in `text`, in bytes, when the parser knows.
    pub span: Option<Range<usize>>,
}

/// The byte range of the character at `index` (counted in characters) of
/// `text`; empty at the end of the text.
pub(crate) fn char_span(text: &str, index: usize) -> Range<usize> {
    let start = text
        .char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte);
    let end = text[start..]
        .chars()
        .next()
        .map_or(start, |c| start + c.len_utf8());
    start..end
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, error) => write!(f, "{}: {error}", path.display()),
            Self::Source(error) => write!(f, "{}: {}", error.path.display(), error.message),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}
