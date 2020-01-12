/// A cache to avoid unnecessary requests.
///
/// The [`Cache`] trait only cares about whether an entry is valid or not.
/// You'll need to implement things like cache invalidation internally.
pub trait Cache: Sync {
    /// Checks if a particular URL is valid.
    fn is_valid(&self, url: &str) -> Option<bool>;
    /// Update the cache entry for `url`.
    fn insert(&self, url: &str, is_valid: bool);
}

impl<'c, C: Cache> Cache for &'c C {
    fn is_valid(&self, url: &str) -> Option<bool> { (**self).is_valid(url) }

    fn insert(&self, url: &str, is_valid: bool) {
        (**self).insert(url, is_valid);
    }
}
