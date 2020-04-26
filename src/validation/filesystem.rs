use crate::validation::Reason;
use std::{
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
};

/// Try to resolve a link relative to the current directory.
///
/// # Note
///
/// The behaviour of this function may vary greatly depending on the
/// [`Options`] passed in.
///
/// ## Root Directory
///
/// Setting a value for [`Options::root_directory()`] acts as a sort of sanity
/// check to prevent links from going outside of a directory tree. It can be
/// useful for preventing [directory traversal attacks][dta] and detecting
/// brittle code (links that go outside of a specific directory may not exist on
/// other machines).
///
/// When the link is absolute, it will be resolved relative to
/// [`Options::root_directory()`]. If now root directory was provided, it will
/// always trigger a [`Reason::TraversesParentDirectories`] error to
/// prevent possible directory traversal attacks.
///
/// ## Default File
///
/// Because a link can only point to a file, when a link specifies a directory
/// we'll automatically append [`Options::default_file()`] to the end.
///
/// This will typically be something like `"index.html"`, meaning a link to
/// `./whatever/` will be resolved to `./whatever/index.html`, which is the
/// default behaviour for web browsers.
///
/// [dta]: https://en.wikipedia.org/wiki/Directory_traversal_attack
pub fn resolve_link(
    current_directory: &Path,
    link: &Path,
    options: &Options,
) -> Result<PathBuf, Reason> {
    let joined = options.join(current_directory, link)?;

    let canonical = options.canonicalize(&joined)?;
    options.sanity_check(&canonical)?;

    if canonical.exists() {
        Ok(canonical)
    } else {
        Err(Reason::Io(std::io::ErrorKind::NotFound.into()))
    }
}

/// Options to be used with [`resolve_link()`].
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    root_directory: Option<PathBuf>,
    default_file: OsString,
}

impl Options {
    pub fn new() -> Self {
        Options {
            root_directory: None,
            default_file: OsString::from("index.html"),
        }
    }

    /// Get the root directory, if one was provided.
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

    /// The default file name to use when a directory is linked to.
    pub fn default_file(&self) -> &OsStr { &self.default_file }

    /// Set the [`Options::default_file()`].
    pub fn set_default_file<O: Into<OsString>>(self, default_file: O) -> Self {
        Options {
            default_file: default_file.into(),
            ..self
        }
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

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Reason> {
        let mut canonical = path.canonicalize()?;

        if canonical.is_dir() {
            canonical.push(&self.default_file);
        }

        Ok(canonical)
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

    fn touch<S: AsRef<Path>>(filename: S, directories: &[&Path]) {
        for dir in directories {
            std::fs::create_dir_all(dir).unwrap();

            let item = dir.join(filename.as_ref());
            let _f = std::fs::File::create(&item).unwrap();
        }
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
        let options =
            Options::default().with_root_directory(temp.path()).unwrap();
        touch(&options.default_file, &[temp.path(), &foo, &bar, &baz]);
        let current_dir = baz.as_path();
        let resolve = |link: &str| -> Result<PathBuf, Reason> {
            resolve_link(current_dir, Path::new(link), &options)
        };

        assert_eq!(
            resolve(".").unwrap(),
            current_dir.join(&options.default_file)
        );
        assert_eq!(resolve("..").unwrap(), bar.join(&options.default_file));
        assert_eq!(resolve("../..").unwrap(), foo.join(&options.default_file));
        assert_eq!(
            resolve("../../..").unwrap(),
            temp.path().join(&options.default_file)
        );
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
        let options =
            Options::default().with_root_directory(temp.path()).unwrap();
        touch(&options.default_file, &[temp.path(), &foo, &bar]);
        let link = Path::new("/bar");

        let got = resolve_link(&foo, link, &options).unwrap();

        assert_eq!(got, bar.join(&options.default_file));
    }
}
