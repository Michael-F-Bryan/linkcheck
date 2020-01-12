//! A library for extracting and verifying links.

mod cache;
pub mod extract;
pub mod verify;

pub use cache::Cache;

use codespan::{FileId, Span};

/// The location for something in its source text.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct Location {
    pub file: FileId,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    text: String,
    href: String,
}

impl Link {
    pub fn new(text: String, href: String) -> Self { Link { text, href } }

    /// The link text.
    pub fn text(&self) -> &str { &self.text }

    /// The raw URL used by this link.
    pub fn href(&self) -> &str { &self.href }
}
