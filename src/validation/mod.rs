mod filesystem;
mod web;

pub use filesystem::{resolve_link, Options};

/// Possible reasons for a bad link.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Reason {
    #[error("Linking outside of the book directory is forbidden")]
    TraversesParentDirectories,
    #[error("An OS-level error occurred")]
    Io(#[from] std::io::Error),
    #[error("The web client encountered an error")]
    Web(#[from] reqwest::Error),
}

impl Reason {
    pub fn file_not_found(&self) -> bool {
        match self {
            Reason::Io(e) => e.kind() == std::io::ErrorKind::NotFound,
            _ => false,
        }
    }

    pub fn timed_out(&self) -> bool {
        match self {
            Reason::Web(e) => e.is_timeout(),
            _ => false,
        }
    }
}
