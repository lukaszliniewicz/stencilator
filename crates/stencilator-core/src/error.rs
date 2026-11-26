//! Errors, with messages written for the person using the tool.

use std::fmt;

/// Why the engine could not do what was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The settings cannot produce a tile grid.
    Layout(String),
    /// A setting is out of range.
    Settings(String),
    /// The image could not be read.
    Image(String),
    /// The chart cannot be turned into a stencil.
    Stencil(String),
    /// The result cannot be written in the requested format.
    Export(String),
}

impl Error {
    /// The message alone.
    pub fn message(&self) -> &str {
        match self {
            Error::Layout(m) | Error::Settings(m) | Error::Image(m) | Error::Stencil(m) | Error::Export(m) => m,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for Error {}
