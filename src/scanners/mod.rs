//! A *scanner* is just a function that which can extract links from a body of
//! text.

#[cfg(feature = "markdown")]
mod markdown;
#[cfg(feature = "plaintext")]
mod plaintext;

#[cfg(feature = "markdown")]
pub use markdown::{markdown, markdown_with_broken_link_callback};
#[cfg(feature = "plaintext")]
pub use plaintext::plaintext;
