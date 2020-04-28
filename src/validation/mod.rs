mod filesystem;
mod web;

pub use filesystem::{check_filesystem, resolve_link, Options};
pub use web::{check_web, get};

use crate::{Category, Link};
use futures::{Future, StreamExt};
use reqwest::{header::HeaderMap, Client, Url};
use std::{
    collections::HashMap,
    path::Path,
    sync::MutexGuard,
    time::{Duration, SystemTime},
};

/// Possible reasons for a bad link.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Reason {
    #[error("Linking outside of the book directory is forbidden")]
    TraversesParentDirectories,
    #[error("An OS-level error occurred")]
    Io(#[from] std::io::Error),
    #[error("The web client encountered an error")]
    Web(#[from] reqwest::Error),
}

impl Reason {
    pub fn file_not_found(&self) -> bool {
        match self {
            Reason::Io(e) => e.kind() == std::io::ErrorKind::NotFound,
            _ => false,
        }
    }

    pub fn timed_out(&self) -> bool {
        match self {
            Reason::Web(e) => e.is_timeout(),
            _ => false,
        }
    }
}

/// Contextual information that callers can provide to guide the validation
/// process.
pub trait Context {
    /// The HTTP client to use.
    fn client(&self) -> &Client;

    /// Options to use when checking a link on the filesystem.
    fn filesystem_options(&self) -> &Options;

    /// Get any extra headers that should be sent when checking this [`Url`].
    fn url_specific_headers(&self, _url: &Url) -> HeaderMap { HeaderMap::new() }

    /// An optional cache that can be used to avoid unnecessary network
    /// requests.
    ///
    /// We need to use internal mutability here because validation is done
    /// concurrently. This [`MutexGuard`] is guaranteed to be short lived (just
    /// the duration of a [`Cache::insert()`] or [`Cache::lookup()`]), so it's
    /// okay to use a [`std::sync::Mutex`] instead of [`futures::lock::Mutex`].
    fn cache(&self) -> Option<MutexGuard<Cache>> { None }

    /// How many items should we check at a time?
    fn concurrency(&self) -> usize { 64 }

    /// How long should a cached item be considered valid for before we need to
    /// check again?
    fn cache_timeout(&self) -> Duration {
        // 24 hours should be a good default
        Duration::from_secs(24 * 60 * 60)
    }

    /// Should this [`Link`] be skipped?
    fn should_ignore(&self, _link: &Link) -> bool { false }
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
    C: Context,
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
    C: Context,
{
    if ctx.should_ignore(&link) {
        log::debug!("Ignoring \"{}\"", link.href);
        return Outcome::Ignored(link);
    }

    match link.category() {
        Some(Category::FileSystem { path, query }) => Outcome::from_result(
            link,
            check_filesystem(current_directory, &path, query, ctx),
        ),
        Some(Category::Url(url)) => {
            Outcome::from_result(link, check_web(&url, ctx).await)
        },
        None => Outcome::UnknownCategory(link),
    }
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

#[derive(Debug, Default)]
pub struct Outcomes {
    pub valid: Vec<Link>,
    pub invalid: Vec<InvalidLink>,
    pub ignored: Vec<Link>,
    pub unknown_category: Vec<Link>,
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

#[derive(Debug)]
pub struct InvalidLink {
    pub link: Link,
    pub reason: Reason,
}

/// A cache used to skip unnecessary network requests.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Cache {
    entries: HashMap<Url, CacheEntry>,
}

impl Cache {
    /// Create a new, empty [`Cache`].
    pub fn new() -> Self { Cache::default() }

    pub fn lookup(&self, url: &Url) -> Option<&CacheEntry> {
        self.entries.get(url)
    }

    /// Add a new [`CacheEntry`] to the cache.
    pub fn insert(&mut self, url: Url, entry: CacheEntry) {
        self.entries.insert(url, entry);
    }

    /// Ask the [`Cache`] whether a particular [`Url`] is still okay (i.e.
    /// [`CacheEntry::success`] is `true`).
    pub fn url_is_still_valid(&self, url: &Url, timeout: Duration) -> bool {
        if let Some(entry) = self.lookup(url) {
            if entry.success {
                if let Ok(time_since_check_was_done) = entry.timestamp.elapsed()
                {
                    return time_since_check_was_done < timeout;
                }
            }
        }

        false
    }

    /// Iterate over all known [`CacheEntries`][CacheEntry], regardless of
    /// whether they are stale or invalid.
    pub fn entries(&self) -> impl Iterator<Item = (&Url, &CacheEntry)> + '_ {
        self.entries.iter()
    }

    /// Forget all [`CacheEntries`][CacheEntry].
    pub fn clear(&mut self) { self.entries.clear(); }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct CacheEntry {
    pub timestamp: SystemTime,
    pub success: bool,
}

impl CacheEntry {
    pub const fn new(timestamp: SystemTime, success: bool) -> Self {
        CacheEntry { timestamp, success }
    }
}
