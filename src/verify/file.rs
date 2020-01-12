use crate::{
    verify::{ValidationResult, Verifier},
    Link,
};

/// A [`Verifier`] for links on the local filesystem.
#[derive(Debug, Clone, PartialEq)]
pub struct File {}

impl Verifier for File {
    fn verify(&self, _link: &Link) -> ValidationResult { unimplemented!() }
}
