//! A library for extracting and verifying links.

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations)]

mod cache;
mod extract;
mod verify;

pub use cache::Cache;
pub use extract::*;
pub use verify::*;

use codespan::{FileId, Span};

/// The location for something in its source text.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct Location {
    /// The file for the item's source code.
    pub file: FileId,
    /// The byte-span within the srouce text.
    pub span: Span,
}

/// A link.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    text: String,
    href: String,
}

impl Link {
    /// Create a new [`Link`].
    pub fn new(text: String, href: String) -> Self { Link { text, href } }

    /// The link text.
    pub fn text(&self) -> &str { &self.text }

    /// The raw URL used by this link.
    pub fn href(&self) -> &str { &self.href }
}
