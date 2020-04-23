//! A *scanner* is just a function that which can extract links from a body of
//! text.

#[cfg(feature = "markdown")]
mod markdown;
#[cfg(feature = "plaintext")]
mod plaintext;

#[cfg(feature = "markdown")]
pub use markdown::{markdown, markdown_with_broken_link_callback};
#[cfg(feature = "plaintext")]
pub use plaintext::plaintext;

use codespan::{FileId, Span};

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Link {
    pub href: String,
    pub span: Span,
    pub file: FileId,
}

/// Use a *scanner* to extract all the links from some text.
pub fn links<S, I>(
    src: &str,
    file: FileId,
    scanner: S,
) -> impl Iterator<Item = Link>
where
    S: FnOnce(&str) -> I,
    I: Iterator<Item = (String, Span)>,
{
    scanner(src).map(move |(href, span)| Link { href, span, file })
}
