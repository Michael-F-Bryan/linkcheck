//! Link verification.

mod file;
mod web;

pub use file::File;
pub use web::Web;

use crate::{Cache, Link, Location};
use rayon::{iter::ParallelBridge, prelude::*};

/// Something used to check whether a link is valid.
pub trait Verifier: Sync {
    /// Check the link.
    ///
    /// # Note to Implementors
    ///
    /// If a particular type of [`Link`] isn't supported, you should quickly
    /// detect this and return [`ValidationResult::Unsupported`] so another
    /// [`Verifier`] can be tried.
    fn verify(&self, link: &Link) -> ValidationResult;
}

impl<F> Verifier for F
where
    F: Fn(&Link) -> ValidationResult,
    F: Sync,
{
    fn verify(&self, link: &Link) -> ValidationResult { self(link) }
}

/// The result of checking if a [`Link`] is valid.
#[derive(Debug)]
pub enum ValidationResult {
    /// This [`Link`] is valid.
    Valid,
    /// This type of link isn't supported.
    Unsupported,
    /// The link should be ignored.
    Ignored,
}

/// A table containing all the valid, invalid, and ignored links.
#[derive(Debug, Default)]
pub struct Outcome {}

impl Outcome {
    /// Merge two [`Outcome`]s.
    pub fn merge(_left: Outcome, _right: Outcome) -> Outcome {
        unimplemented!()
    }

    fn with_result(
        self,
        _location: Location,
        _link: Link,
        _result: ValidationResult,
    ) -> Outcome {
        unimplemented!()
    }
}

impl FromParallelIterator<(Location, Link, ValidationResult)> for Outcome {
    fn from_par_iter<I>(par_iter: I) -> Self
    where
        I: IntoParallelIterator<Item = (Location, Link, ValidationResult)>,
    {
        par_iter
            .into_par_iter()
            .fold(Outcome::default, |outcome, (location, link, result)| {
                outcome.with_result(location, link, result)
            })
            .reduce(Outcome::default, Outcome::merge)
    }
}

/// Attempt to verify a set of [`Link`]s in parallel.
pub fn verify<L, C>(
    links: L,
    verifiers: &[Box<dyn Verifier>],
    cache: &dyn Cache,
) -> Outcome
where
    L: IntoIterator<Item = (Location, Link)>,
    L::IntoIter: Send,
    L::Item: Send,
{
    links
        .into_iter()
        .par_bridge()
        .map(|(location, link)| {
            let result = verify_one(&link, verifiers, cache);
            (location, link, result)
        })
        .collect()
}

fn verify_one(
    link: &Link,
    verifiers: &[Box<dyn Verifier>],
    cache: &dyn Cache,
) -> ValidationResult {
    if cache.is_valid(link.href()).unwrap_or(false) {
        log::debug!("Cache hit for \"{}\"", link.href());
        return ValidationResult::Valid;
    }

    for verifier in verifiers {
        match verifier.verify(link) {
            ValidationResult::Unsupported => continue,
            other => return other,
        }
    }

    log::debug!("No verifiers were able to handle \"{}\"", link.href());
    ValidationResult::Unsupported
}
