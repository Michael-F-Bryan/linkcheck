use codespan::Span;
use pulldown_cmark::{BrokenLink, CowStr, Event, Options, Parser, Tag};

/// A scanner that uses [`pulldown_cmark`] to extract all links from markdown.
///
/// # Examples
///
/// ```rust
/// # use codespan::Span;
/// let src = "This is a [link](https://example.com/) and an ![Image](img.png)";
///
/// let got: Vec<_> = linkcheck::scanners::markdown(src).collect();
///
/// assert_eq!(got.len(), 2);
/// let (href, span) = &got[0];
/// assert_eq!(href, "https://example.com/");
/// assert_eq!(*span, Span::new(10, 38));
/// ```
pub fn markdown(src: &str) -> impl Iterator<Item = (String, Span)> + '_ {
    markdown_with_broken_link_callback(src, None)
}

/// The callback passed to `pulldown-cmark` whenever a broken link is
/// encountered.
pub type BrokenLinkCallback<'src> = dyn FnMut(BrokenLink<'_>) -> std::option::Option<(CowStr<'src>, CowStr<'src>)>
    + 'src;

/// A scanner that uses [`pulldown_cmark`] to extract all links from markdown,
/// using the supplied callback to try and fix broken links.
pub fn markdown_with_broken_link_callback<'a>(
    src: &'a str,
    on_broken_link: Option<&'a mut BrokenLinkCallback<'a>>,
) -> impl Iterator<Item = (String, Span)> + 'a {
    Parser::new_with_broken_link_callback(
        src,
        Options::ENABLE_FOOTNOTES,
        on_broken_link,
    )
    .into_offset_iter()
    .filter_map(|(event, range)| match event {
        Event::Start(Tag::Link(_, dest, _))
        | Event::Start(Tag::Image(_, dest, _)) => Some((
            dest.to_string(),
            Span::new(range.start as u32, range.end as u32),
        )),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_common_links_in_markdown() {
        let src = r#"
# Some Heading

[this](https://example.com) is a link [to nowhere][nowhere]. But
[this](../README.md) points somewhere on disk.

![Look, an image!](https://imgur.com/gallery/f28OkrB)

[nowhere]: https://dev.null/

- [x] Comments
- [ ] Issues

```sql
ALTER FOREIGN TABLE [ IF EXISTS ] [ ONLY ] name [ * ]
    action [, ... ]
```

        "#;
        let should_be = vec![
            (String::from("https://example.com"), Span::new(17, 44)),
            (String::from("https://dev.null/"), Span::new(55, 76)),
            (String::from("../README.md"), Span::new(82, 102)),
            (
                String::from("https://imgur.com/gallery/f28OkrB"),
                Span::new(130, 183),
            ),
        ];

        let got: Vec<_> =
            markdown_with_broken_link_callback(src, Some(&mut |_| None)).collect();

        assert_eq!(got, should_be);
    }

    #[test]
    fn detect_broken_links_in_markdown() {
        let src = r#"
# Some Heading

[this](https://example.com) is a link [to nowhere][nowhere]. But
[this](../README.md) points somewhere on disk.

![Look, an image!](https://imgur.com/gallery/f28OkrB)

[nowhere]: https://dev.null/

- [x] Comments
- [ ] Issues

```sql
ALTER FOREIGN TABLE [ IF EXISTS ] [ ONLY ] name [ * ]
    action [, ... ]
```

        "#;
        let should_be = vec![
            (String::from("https://example.com"), Span::new(17, 44)),
            (String::from("https://dev.null/"), Span::new(55, 76)),
            (String::from("../README.md"), Span::new(82, 102)),
            (
                String::from("https://imgur.com/gallery/f28OkrB"),
                Span::new(130, 183),
            ),
        ];

        let got: Vec<_> = markdown_with_broken_link_callback(src, &|a, b| Some((a.to_string(), b.to_string()))).collect();

        assert_eq!(got, should_be);
    }
}
