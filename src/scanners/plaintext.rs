use codespan::Span;
use linkify::{LinkFinder, LinkKind};

/// Use the [`linkify`] crate to find all URLs in a string of normal text.
///
/// # Examples
///
/// ```rust
/// # use codespan::Span;
/// let src = "hello http://localhost/ world. this is file://some/text";
///
/// let got: Vec<_> = linkcheck::scanners::plaintext(src).collect();
///
/// assert_eq!(got.len(), 2);
/// let (url, span) = got[0];
/// assert_eq!(url, "http://localhost/");
/// assert_eq!(span, Span::new(6, 23));
/// ```
pub fn plaintext(src: &str) -> impl Iterator<Item = (&str, Span)> + '_ {
    LinkFinder::new()
        .kinds(&[LinkKind::Url])
        .links(src)
        .map(|link| {
            (
                link.as_str(),
                Span::new(link.start() as u32, link.end() as u32),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_urls_in_some_text() {
        let src = "hello http://localhost/ world. this is file://some/text.";
        let should_be = vec![
            ("http://localhost/", Span::new(6, 23)),
            ("file://some/text", Span::new(39, 55)),
        ];

        let got: Vec<_> = plaintext(src).collect();

        assert_eq!(got, should_be);
    }
}
