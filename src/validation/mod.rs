mod filesystem;
mod web;

pub use filesystem::{resolve_link, Options};

#[derive(Debug, thiserror::Error)]
pub enum Reason {
    #[error("Linking outside of the book directory is forbidden")]
    TraversesParentDirectories,
    #[error("An OS-level error occurred")]
    Io(#[from] std::io::Error),
}
