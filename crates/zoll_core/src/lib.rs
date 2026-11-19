//! The engine of zoll: it walks the files, tells their languages, finds the
//! code embedded in them and evaluates the policies. The `zoll` command is a
//! thin layer over it.

pub mod config;
pub mod document;
pub mod error;
pub mod input;
pub mod language;
pub mod walk;

pub use error::{Error, SourceError};
