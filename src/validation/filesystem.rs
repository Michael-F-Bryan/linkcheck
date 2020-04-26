use crate::validation::Reason;
use std::{
    io,
    path::{Path, PathBuf},
};

/// Try to resolve a link relative to the current directory.
///
/// # Note
///
/// If the link is absolute, the link will be resolved relative to
/// [`Options::root_directory()`]. Not providing a root directory will always
/// trigger a [`Reason::TraversesParentDirectories`] error to prevent possible
/// directory traversal attacks.
pub fn resolve_link(
    current_directory: &Path,
    link: &Path,
    options: &Options,
) -> Result<PathBuf, Reason> {
    let joined = options.join(current_directory, link)?;

    let canonical = match joined.canonicalize() {
        Ok(c) => c,
        Err(e) => return Err(Reason::Io(e)),
    };

    options.sanity_check(&canonical)?;

    Ok(canonical)
}

/// Options to be used with [`resolve_link()`].
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    root_directory: Option<PathBuf>,
}

impl Options {
    pub const fn new() -> Self {
        Options {
            root_directory: None,
        }
    }

    /// Get the root directory, if one was provided.
    ///
    /// This acts as a sort of sanity check to prevent links from going outside
    /// of a directory tree. It can be useful for preventing [directory
    /// traversal attacks][dta] and detecting brittle code (links that go
    /// outside of a specific directory may not exist on other machines).
    ///
    /// [dta]: https://en.wikipedia.org/wiki/Directory_traversal_attack
    pub fn root_directory(&self) -> Option<&Path> {
        self.root_directory.as_ref().map(|p| &**p)
    }

    /// Set the [`Options::root_directory()`], automatically converting to its
    /// canonical form with [`std::fs::canonicalize()`].
    pub fn with_root_directory<P: AsRef<Path>>(
        self,
        root_directory: P,
    ) -> io::Result<Self> {
        Ok(Options {
            root_directory: Some(std::fs::canonicalize(root_directory)?),
            ..self
        })
    }

    fn join(
        &self,
        current_dir: &Path,
        second: &Path,
    ) -> Result<PathBuf, Reason> {
        if second.is_absolute() {
            // if the path is absolute (i.e. has a leading slash) then it's
            // meant to be relative to the root directory, not the current one
            match self.root_directory() {
                Some(root) => {
                    let mut buffer = root.to_path_buf();
                    buffer.extend(second.iter().skip(1));
                    Ok(buffer)
                },
                // You really shouldn't provide links to absolute files on your
                // system (e.g. "/home/michael/Documents/whatever" or
                // "/etc/passwd").
                //
                // For one, it's extremely brittle and will probably only work
                // on that computer, but more importantly it's also a vector
                // for directory traversal attacks.
                //
                // Feel free to send a PR if you believe otherwise.
                None => Err(Reason::TraversesParentDirectories),
            }
        } else {
            Ok(current_dir.join(second))
        }
    }

    fn sanity_check(&self, path: &Path) -> Result<(), Reason> {
        if let Some(root) = self.root_directory() {
            if !path.starts_with(root) {
                return Err(Reason::TraversesParentDirectories);
            }
        }

        Ok(())
    }
}

impl Default for Options {
    fn default() -> Self { Options::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validation_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("validation")
    }

    #[test]
    fn resolve_mod_relative_to_validation_dir() {
        let current_dir = validation_dir();
        let link = "mod.rs";
        let options = Options::default();

        let got =
            resolve_link(&current_dir, Path::new(link), &options).unwrap();

        assert_eq!(got, current_dir.join(link));
    }

    #[test]
    fn detect_possible_directory_traversal_attacks() {
        let temp = tempfile::tempdir().unwrap();
        let foo = temp.path().join("foo");
        let bar = foo.join("bar");
        let baz = bar.join("baz");
        std::fs::create_dir_all(&baz).unwrap();

        let current_dir = baz.as_path();
        let options =
            Options::default().with_root_directory(temp.path()).unwrap();
        let resolve = |link: &str| -> Result<PathBuf, Reason> {
            resolve_link(current_dir, Path::new(link), &options)
        };

        assert_eq!(resolve(".").unwrap(), current_dir);
        assert_eq!(resolve("..").unwrap(), bar);
        assert_eq!(resolve("../..").unwrap(), foo);
        assert_eq!(resolve("../../..").unwrap(), temp.path());
        assert!(matches!(
            resolve("../../../..").unwrap_err(),
            Reason::TraversesParentDirectories
        ));
    }

    #[test]
    fn links_with_a_leading_slash_are_relative_to_the_root() {
        let temp = tempfile::tempdir().unwrap();
        let foo = temp.path().join("foo");
        let bar = temp.path().join("bar");
        std::fs::create_dir_all(&foo).unwrap();
        std::fs::create_dir_all(&bar).unwrap();
        let options =
            Options::default().with_root_directory(temp.path()).unwrap();
        let link = Path::new("/bar");

        let got = resolve_link(&foo, link, &options).unwrap();

        assert_eq!(got, bar);
    }
}
