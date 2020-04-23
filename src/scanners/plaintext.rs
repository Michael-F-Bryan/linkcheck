use crate::codespan::Span;
use regex::Regex;

pub fn plaintext(src: &str) -> impl Iterator<Item = (&str, Span)> + '_ {
    URL_REGEX.find_iter(src)
        .map(|cap| (cap.as_str(), Span::new(cap.start() as u32, cap.end() as u32)))
}

lazy_static::lazy_static! {
    static ref URL_REGEX: Regex = Regex::new(r#"(?x)
        \b
        (http(s)?://)?
        \w+(\.\w+)?
        \b
    "#).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_urls_in_some_text() {
        let src = "hello http://localhost/ world. this is ./some/text";
        let should_be = vec![
            ("http://localhost/", Span::new(0, 0)),
            ("./some/text", Span::new(0, 0)),
        ];

        let got: Vec<_> = plaintext(src).collect();

        assert_eq!(got, should_be);
    }
}
