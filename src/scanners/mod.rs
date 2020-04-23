//! A *scanner* is just a function that which can extract links from a body of
//! text.

use codespan::{FileId, Span};

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Link {
    pub href: String,
    pub span: Span,
    pub file: FileId,
}

/// Use a *scanner* to extract all the links from some text.
pub fn links<'a, S, I>(
    src: &'a str,
    file: FileId,
    scanner: S,
) -> impl Iterator<Item = Link> + 'a
where
    S: FnOnce(&'a str) -> I,
    I: Iterator<Item = (&'a str, Span)> + 'a,
{
    scanner(src).map(move |(href, span)| Link {
        href: href.to_string(),
        span,
        file,
    })
}
