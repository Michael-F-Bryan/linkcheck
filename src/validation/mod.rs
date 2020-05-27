//! Code for validating the various types of [`Link`].

mod cache;
mod context;
mod filesystem;
mod web;

pub use cache::{Cache, CacheEntry};
pub use context::{BasicContext, Context};
pub use filesystem::{check_filesystem, resolve_link, Options};
pub use web::{check_web, get};

use crate::{Category, Link};
use futures::{Future, StreamExt};
use std::path::Path;

/// Possible reasons for a bad link.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Reason {
    /// The link goes outside of the root directory.
    #[error("Linking outside of the \"root\" directory is forbidden")]
    TraversesParentDirectories,
    /// The OS returned an error (e.g. file not found).
    #[error("An OS-level error occurred")]
    Io(#[from] std::io::Error),
    /// The HTTP client returned an error.
    #[error("The web client encountered an error")]
    Web(#[from] reqwest::Error),
}

impl Reason {
    /// Was this failure due to a missing file?
    pub fn file_not_found(&self) -> bool {
        match self {
            Reason::Io(e) => e.kind() == std::io::ErrorKind::NotFound,
            _ => false,
        }
    }

    /// Did the HTTP client time out?
    pub fn timed_out(&self) -> bool {
        match self {
            Reason::Web(e) => e.is_timeout(),
            _ => false,
        }
    }
}

/// Validate several [`Link`]s relative to a particular directory.
pub fn validate<'a, L, C>(
    current_directory: &'a Path,
    links: L,
    ctx: &'a C,
) -> impl Future<Output = Outcomes> + 'a
where
    L: IntoIterator<Item = Link>,
    L::IntoIter: 'a,
    C: Context + ?Sized,
{
    futures::stream::iter(links)
        .map(move |link| validate_one(link, current_directory, ctx))
        .buffer_unordered(ctx.concurrency())
        .collect()
}

/// Try to validate a single link, deferring to the appropriate validator based
/// on the link's [`Category`].
async fn validate_one<C>(
    link: Link,
    current_directory: &Path,
    ctx: &C,
) -> Outcome
where
    C: Context + ?Sized,
{
    if ctx.should_ignore(&link) {
        log::debug!("Ignoring \"{}\"", link.href);
        return Outcome::Ignored(link);
    }

    match link.category() {
        Some(Category::FileSystem { path, fragment }) => Outcome::from_result(
            link,
            check_filesystem(
                current_directory,
                &path,
                fragment.as_deref(),
                ctx,
            ),
        ),
        Some(Category::CurrentFile { fragment }) => {
            // TODO: How do we want to validate links to other parts of the
            // current file?
            //
            // It seems wasteful to go through the whole filesystem resolution
            // process when the filename was recorded when adding its text to
            // `Files`... Maybe we could thread `Files` through and then join it
            // with `ctx.filesystem_options().root_directory()`?
            log::warn!("Not checking \"{}\" in the current file because fragment resolution isn't implemented", fragment);
            Outcome::Ignored(link)
        },
        Some(Category::Url(url)) => {
            Outcome::from_result(link, check_web(&url, ctx).await)
        },
        None => Outcome::UnknownCategory(link),
    }
}

/// The result of validating a batch of [`Link`]s.
#[derive(Debug, Default)]
pub struct Outcomes {
    /// Valid links.
    pub valid: Vec<Link>,
    /// Links which are broken.
    pub invalid: Vec<InvalidLink>,
    /// Items that were explicitly ignored by the [`Context`].
    pub ignored: Vec<Link>,
    /// Links which we weren't able to identify a suitable validator for.
    pub unknown_category: Vec<Link>,
}

impl Outcomes {
    /// Create an empty set of [`Outcomes`].
    pub fn empty() -> Self { Outcomes::default() }

    /// Merge two [`Outcomes`].
    pub fn merge(&mut self, other: Outcomes) {
        self.valid.extend(other.valid);
        self.invalid.extend(other.invalid);
        self.ignored.extend(other.ignored);
        self.unknown_category.extend(other.unknown_category);
    }
}

impl Extend<Outcome> for Outcomes {
    fn extend<T: IntoIterator<Item = Outcome>>(&mut self, items: T) {
        for outcome in items {
            match outcome {
                Outcome::Valid(v) => self.valid.push(v),
                Outcome::Invalid(i) => self.invalid.push(i),
                Outcome::Ignored(i) => self.ignored.push(i),
                Outcome::UnknownCategory(u) => self.unknown_category.push(u),
            }
        }
    }
}

impl Extend<Outcomes> for Outcomes {
    fn extend<T: IntoIterator<Item = Outcomes>>(&mut self, items: T) {
        for item in items {
            self.merge(item);
        }
    }
}

/// A [`Link`] and the [`Reason`] why it is invalid.
#[derive(Debug)]
pub struct InvalidLink {
    /// The invalid link.
    pub link: Link,
    /// Why is this link invalid?
    pub reason: Reason,
}

#[derive(Debug)]
enum Outcome {
    Valid(Link),
    Invalid(InvalidLink),
    Ignored(Link),
    UnknownCategory(Link),
}

impl Outcome {
    fn from_result<T, E>(link: Link, result: Result<T, E>) -> Self
    where
        E: Into<Reason>,
    {
        match result {
            Ok(_) => Outcome::Valid(link),
            Err(e) => Outcome::Invalid(InvalidLink {
                link,
                reason: e.into(),
            }),
        }
    }
}
