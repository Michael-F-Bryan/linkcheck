//! A *scanner* is just a function that which can extract links from a body of
//! text.

mod markdown;
mod plaintext;

pub use markdown::{markdown, markdown_with_broken_link_callback};
pub use plaintext::plaintext;
