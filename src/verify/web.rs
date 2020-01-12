use crate::{
    verify::{ValidationResult, Verifier},
    Link,
};

/// A [`Verifier`] for checking links on the internet.
#[derive(Debug, Clone, PartialEq)]
pub struct Web {}

impl Verifier for Web {
    fn verify(&self, _link: &Link) -> ValidationResult { unimplemented!() }
}
