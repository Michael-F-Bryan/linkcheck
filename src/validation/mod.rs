//! Code for validating the various types of [`Link`].

mod cache;
mod filesystem;
mod web;

pub use cache::{Cache, CacheEntry};
pub use filesystem::{check_filesystem, resolve_link, Options};
pub use web::{check_web, get};

use crate::{Category, Link};
use futures::{Future, StreamExt};
use reqwest::{header::HeaderMap, Client, Url};
use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

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
        let Outcomes {
            valid,
            invalid,
            ignored,
            unknown_category,
        } = other;
        self.valid.extend(valid);
        self.invalid.extend(invalid);
        self.ignored.extend(ignored);
        self.unknown_category.extend(unknown_category);
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

/// A basic [`Context`] implementation which uses all the defaults.
#[derive(Debug)]
pub struct BasicContext {
    client: Client,
    options: Options,
    cache: Mutex<Cache>,
}

impl BasicContext {
    /// The User-Agent used by the [`BasicContext::client()`].
    pub const USER_AGENT: &'static str =
        concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

    /// Create a [`BasicContext`] with an already initialized [`Client`].
    pub fn with_client(client: Client) -> Self {
        BasicContext {
            client,
            options: Options::default(),
            cache: Mutex::new(Cache::new()),
        }
    }

    /// Get a mutable reference to the [`Options`] used when validating
    /// filesystem links.
    pub fn options_mut(&mut self) -> &mut Options { &mut self.options }
}

impl Default for BasicContext {
    fn default() -> Self {
        let client = Client::builder()
            .user_agent(BasicContext::USER_AGENT)
            .build()
            .expect("Unable to initialize the client");

        BasicContext::with_client(client)
    }
}

impl Context for BasicContext {
    fn client(&self) -> &Client { &self.client }

    fn filesystem_options(&self) -> &Options { &self.options }

    fn cache(&self) -> Option<MutexGuard<Cache>> {
        Some(self.cache.lock().expect("Mutex was poisoned"))
    }
}
