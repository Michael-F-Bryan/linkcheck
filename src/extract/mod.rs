//! Link extraction.

use crate::Link;
use codespan::Span;

/// Something that can be used to extract links from some source text.
pub trait LinkExtractor {
    fn extract(&self, src: &str) -> Vec<(Link, Span)>;
}

impl<F> LinkExtractor for F
where
    F: Fn(&str) -> Vec<(Link, Span)>,
{
    fn extract(&self, src: &str) -> Vec<(Link, Span)> { self(src) }
}
