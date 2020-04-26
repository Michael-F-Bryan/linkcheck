#![forbid(unsafe_code)]

pub extern crate codespan;

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

pub mod scanners;
pub mod validation;

use codespan::{FileId, Span};
use reqwest::Url;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Link {
    pub href: String,
    pub span: Span,
    pub file: FileId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Category {
    /// A local file.
    FileSystem { path: PathBuf, query: String },
    /// A URL for something on the web.
    Url(Url),
}
