use crate::validation::{Context, Reason};
use std::{
    collections::HashMap,
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
/// Setting a value for [`Options::root_directory()`] and
/// [`Options::links_may_traverse_the_root_directory()`] act as a sort of sanity
/// check to prevent links from going outside of a directory tree. They can also
/// be useful in preventing [directory traversal attacks][dta] and detecting
/// brittle code (links that go outside of a specific directory may not exist on
/// other machines).
///
/// When the link is absolute, it will be resolved relative to
/// [`Options::root_directory()`]. If now root directory was provided, it will
/// *always* trigger a [`Reason::TraversesParentDirectories`] error to
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

    // Note: canonicalizing also made sure the file exists
    Ok(canonical)
}

/// Check whether a [`Path`] points to a valid file on disk.
///
/// If a fragment specifier is provided, this function will scan through the
/// linked document and check that the file contains the corresponding anchor
/// (e.g. markdown heading or HTML `id`).
pub fn check_filesystem<C>(
    current_directory: &Path,
    path: &Path,
    fragment: Option<&str>,
    ctx: &C,
) -> Result<(), Reason>
where
    C: Context + ?Sized,
{
    log::debug!(
        "Checking \"{}\" in the context of \"{}\"",
        path.display(),
        current_directory.display()
    );

    let resolved_location =
        resolve_link(current_directory, path, ctx.filesystem_options())?;

    log::debug!(
        "\"{}\" resolved to \"{}\"",
        path.display(),
        resolved_location.display()
    );

    if let Some(fragment) = fragment {
        // TODO: detect the file type and check the fragment exists
        log::warn!(
            "Not checking that the \"{}\" section exists in \"{}\" because fragment resolution isn't implemented",
            fragment,
            resolved_location.display(),
        );
    }

    Ok(())
}

/// Options to be used with [`resolve_link()`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde-1", derive(serde::Serialize, serde::Deserialize))]
pub struct Options {
    root_directory: Option<PathBuf>,
    default_file: OsString,
    links_may_traverse_the_root_directory: bool,
    alternate_extensions: HashMap<OsString, Vec<OsString>>,
}

impl Options {
    /// A mapping of possible alternate extensions to try when checking a
    /// filesystem link.
    pub const DEFAULT_ALTERNATE_EXTENSIONS: &'static [(
        &'static str,
        &'static [&'static str],
    )] = &[("md", &["html"])];
    /// The name used by [`Options::default_file()`].
    pub const DEFAULT_FILE: &'static str = "index.html";

    /// Create a new [`Options`] populated with some sane defaults.
    pub fn new() -> Self {
        Options {
            root_directory: None,
            default_file: OsString::from(Options::DEFAULT_FILE),
            links_may_traverse_the_root_directory: false,
            alternate_extensions: Options::DEFAULT_ALTERNATE_EXTENSIONS
                .iter()
                .map(|(ext, alts)| {
                    (
                        OsString::from(ext),
                        alts.iter().map(OsString::from).collect(),
                    )
                })
                .collect(),
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

    /// Get the map of alternate extensions to use when checking.
    ///
    /// By default we only map `*.md` to `*.html`
    /// ([`Options::DEFAULT_ALTERNATE_EXTENSIONS`]).
    pub fn alternate_extensions(
        &self,
    ) -> impl Iterator<Item = (&OsStr, impl Iterator<Item = &OsStr>)> {
        self.alternate_extensions.iter().map(|(key, value)| {
            (key.as_os_str(), value.iter().map(|alt| alt.as_os_str()))
        })
    }

    /// Set the [`Options::alternate_extensions()`] mapping.
    pub fn set_alternate_extensions<S, I, V>(mut self, alternates: I) -> Self
    where
        I: IntoIterator<Item = (S, V)>,
        S: Into<OsString>,
        V: IntoIterator<Item = S>,
    {
        let mut mapping = HashMap::new();

        for (ext, alts) in alternates {
            mapping
                .insert(ext.into(), alts.into_iter().map(Into::into).collect());
        }

        self.alternate_extensions = mapping;

        self
    }

    /// Are links allowed to go outside of the [`Options::root_directory()`]?
    pub fn links_may_traverse_the_root_directory(&self) -> bool {
        self.links_may_traverse_the_root_directory
    }

    /// Set [`Options::links_may_traverse_the_root_directory()`].
    pub fn set_links_may_traverse_the_root_directory(
        self,
        value: bool,
    ) -> Self {
        Options {
            links_may_traverse_the_root_directory: value,
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
                    // append everything except the root element
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
            // we need to canonicalize again because the default file may be a
            // symlink, or not exist at all
            canonical = canonical.canonicalize()?;
        }

        Ok(canonical)
    }

    fn sanity_check(&self, path: &Path) -> Result<(), Reason> {
        if let Some(root) = self.root_directory() {
            if !(self.links_may_traverse_the_root_directory
                || path.starts_with(root))
            {
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

        // checking up to the root directory is okay
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
        // but a directory traversal attack isn't
        assert!(matches!(
            resolve(
                "../../../../../../../../../../../../../../../../../etc/passwd"
            )
            .unwrap_err(),
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

    #[test]
    fn link_to_a_file_we_know_doesnt_exist() {
        let temp = tempfile::tempdir().unwrap();
        let options =
            Options::default().with_root_directory(temp.path()).unwrap();
        let link = Path::new("./bar");

        let err = resolve_link(temp.path(), link, &options).unwrap_err();

        assert!(err.file_not_found());
    }

    #[test]
    fn absolute_link_with_no_root_set_is_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let options = Options::default();
        let link = Path::new("/bar");

        let err = resolve_link(temp.path(), link, &options).unwrap_err();

        assert!(matches!(err, Reason::TraversesParentDirectories));
    }

    #[test]
    fn a_link_that_is_allowed_to_traverse_the_root_dir() {
        let temp = tempfile::tempdir().unwrap();
        let foo = temp.path().join("foo");
        let bar = temp.path().join("bar");
        touch(Options::DEFAULT_FILE, &[temp.path(), &foo, &bar]);
        let options = Options::default()
            .with_root_directory(&foo)
            .unwrap()
            .set_links_may_traverse_the_root_directory(true);
        let link = Path::new("../bar/index.html");

        let got = resolve_link(&foo, link, &options).unwrap();

        assert_eq!(got, bar.join("index.html"));
    }
}
