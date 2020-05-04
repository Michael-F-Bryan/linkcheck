//! A library for extracting and validating links.
//!
//! The majority of this code has been extracted from the
//! [`mdbook-linkcheck`](https://crates.io/crates/mdbook-linkcheck) plugin, so
//! it may have some bias towards the way `mdbook` works.
//!
//! # Examples
//!
//! If you were validating links in batches, this is one way to go about it:
//!
//! ```rust
//! use linkcheck::{Link, BasicContext};
//! use std::path::Path;
//! use codespan::Files;
//!
//! # #[tokio::main] async fn main() {
//! // first we need somewhere to put the source documents we'll be checking
//! let mut files = Files::new();
//!
//! // then we add some items
//! let src = r#"
//! This is some markdown linking to [a website](https://example.com) and
//! [a file](./README.md).
//! "#;
//! let file_id = files.add("blah.md", src);
//!
//! // we then need to extract all the links and their location in the document
//! let links = linkcheck::scanners::markdown(src);
//!
//! // at the moment we just have a stream of (&str, Span)... To give nice
//! // diagnostics we need to turn this into a stream of Links that know which
//! // document they came from.
//! let links = links.map(|(url, span)| Link::new(url, span, file_id));
//!
//! // we've collected all our links, now it's time for validation!
//!
//! // when validating file links we need to know what the current directory is
//! let current_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
//!
//! // the validation process also need some contextual information (e.g. HTTP
//! // client, file system validation options, and a cache for expensive web
//! // requests).
//! //
//! // Basic users won't need to tweak this in any way, so a default context
//! // type has been provided for you.
//! let ctx = BasicContext::default();
//!
//! // and now we can run the validation step!
//! let result = linkcheck::validate(current_dir, links, &ctx).await;
//!
//! assert!(result.invalid.is_empty());
//! assert_eq!(result.valid.len(), 2);
//! # }
//! ```
//!
//! # Cargo Features
//!
//! Extra functionality is accessible by enabling feature flags. The features
//! currently available are:
//!
//! * **serde-1** - Adds `Serialize` and `Deserialize` implementations for use
//!   with `serde`

#![forbid(unsafe_code)]
#![deny(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations
)]

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

pub mod scanners;
pub mod validation;

pub use validation::{validate, BasicContext};

use codespan::{FileId, Span};
use http::uri::PathAndQuery;
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Category {
    /// A local file.
    FileSystem {
        path: PathBuf,
        fragment: Option<String>,
    },
    /// A URL for something on the web.
    Url(Url),
}

impl Category {
    fn categorise(src: &str) -> Option<Self> {
        if let Ok(url) = src.parse() {
            return Some(Category::Url(url));
        }

        let (path, fragment) = match src.find("#") {
            Some(hash) => {
                let (path, rest) = src.split_at(hash);
                (path, Some(String::from(&rest[1..])))
            },
            None => (src, None),
        };

        // as a sanity check we use the http crate's PathAndQuery type to make
        // sure the path is decoded correctly
        if let Ok(path_and_query) = path.parse::<PathAndQuery>() {
            return Some(Category::FileSystem {
                path: PathBuf::from(path_and_query.path()),
                fragment,
            });
        }

        None
    }
}

/// A link to some other resource.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde-1", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Link {
    /// The link itself.
    pub href: String,
    /// Where the [`Link`] lies in its source text.
    pub span: Span,
    /// Which document does this [`Link`] belong to?
    pub file: FileId,
}

impl Link {
    /// Create a new [`Link`].
    pub fn new<S: Into<String>>(href: S, span: Span, file: FileId) -> Self {
        Link {
            href: href.into(),
            span,
            file,
        }
    }

    fn category(&self) -> Option<Category> { Category::categorise(&self.href) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_into_categories() {
        let inputs = vec![
            (
                "https://example.com/",
                Some(Category::Url(
                    Url::parse("https://example.com/").unwrap(),
                )),
            ),
            (
                "README.md",
                Some(Category::FileSystem {
                    path: PathBuf::from("README.md"),
                    fragment: None,
                }),
            ),
            (
                "./README.md",
                Some(Category::FileSystem {
                    path: PathBuf::from("./README.md"),
                    fragment: None,
                }),
            ),
            (
                "./README.md#license",
                Some(Category::FileSystem {
                    path: PathBuf::from("./README.md"),
                    fragment: Some(String::from("license")),
                }),
            ),
        ];

        for (src, should_be) in inputs {
            let got = Category::categorise(src);
            assert_eq!(got, should_be, "{}", src);
        }
    }
}
