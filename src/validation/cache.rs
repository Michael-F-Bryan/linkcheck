use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};
use url::Url;

/// A cache used to skip unnecessary network requests.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde-1", derive(serde::Serialize, serde::Deserialize))]
pub struct Cache {
    entries: HashMap<Url, CacheEntry>,
}

impl Cache {
    /// Create a new, empty [`Cache`].
    pub fn new() -> Self { Cache::default() }

    /// Lookup a particular [`CacheEntry`].
    pub fn lookup(&self, url: &Url) -> Option<&CacheEntry> {
        self.entries.get(url)
    }

    /// Add a new [`CacheEntry`] to the cache.
    pub fn insert(&mut self, url: Url, entry: CacheEntry) {
        self.entries.insert(url, entry);
    }

    /// Ask the [`Cache`] whether a particular [`Url`] is still okay (i.e.
    /// [`CacheEntry::valid`] is `true`).
    pub fn url_is_still_valid(&self, url: &Url, timeout: Duration) -> bool {
        if let Some(entry) = self.lookup(url) {
            if entry.valid {
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
    pub fn iter(&self) -> impl Iterator<Item = (&Url, &CacheEntry)> + '_ {
        self.entries.iter()
    }

    /// Forget all [`CacheEntries`][CacheEntry].
    pub fn clear(&mut self) { self.entries.clear(); }
}

impl Extend<(Url, CacheEntry)> for Cache {
    fn extend<T: IntoIterator<Item = (Url, CacheEntry)>>(&mut self, iter: T) {
        self.entries.extend(iter);
    }
}

/// A timestamped boolean used by the [`Cache`] to keep track of the last time
/// a web [`crate::Link`] was checked.
#[derive(Debug, Copy, Clone, PartialEq)]
#[cfg_attr(feature = "serde-1", derive(serde::Serialize, serde::Deserialize))]
pub struct CacheEntry {
    /// When the [`CacheEntry`] was created.
    pub timestamp: SystemTime,
    /// Did we find a valid resource the last time this [`crate::Link`] was
    /// checked?
    pub valid: bool,
}

impl CacheEntry {
    /// Create a new [`CacheEntry`].
    pub const fn new(timestamp: SystemTime, valid: bool) -> Self {
        CacheEntry { timestamp, valid }
    }
}
