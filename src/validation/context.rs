use crate::{
    validation::{Cache, Options},
    Link,
};
use reqwest::{header::HeaderMap, Client, Url};
use std::{
    sync::{Mutex, MutexGuard},
    time::Duration,
};

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

/// A basic [`Context`] implementation which uses all the defaults.
#[derive(Debug)]
pub struct BasicContext {
    /// Options used when validating filesystem links.
    pub options: Options,
    client: Client,
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
    #[deprecated = "Access the field directly instead"]
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
