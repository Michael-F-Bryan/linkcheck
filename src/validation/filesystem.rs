use super::path::normalize_path;
use crate::validation::{Context, Reason};
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fmt::{self, Debug, Formatter},
    io,
    path::{Component, Path, PathBuf},
    sync::Arc,
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
/// ## Alternate Extensions
///
/// Sometimes you might have a `index.md` document but also accept `index.html`
/// as a valid link (like in `mdbook`). For this you can provide a mapping of
/// [`Options::alternate_extensions()`] to fall back to when the original
/// extension doesn't work.
///
/// [dta]: https://en.wikipedia.org/wiki/Directory_traversal_attack
pub fn resolve_link(
    current_directory: &Path,
    link: &Path,
    options: &Options,
) -> Result<PathBuf, Reason> {
    let joined = options.join(current_directory, link)?;

    let candidates = options.possible_names(joined);

    for candidate in candidates {
        log::trace!(
            "Checking if \"{}\" points to \"{}\"",
            link.display(),
            candidate.display(),
        );

        if let Ok(canonical) = options.canonicalize(&candidate) {
            options.sanity_check(&canonical)?;
            return Ok(canonical);
        }
    }

    log::trace!("None of the candidates exist for \"{}\"", link.display());
    Err(Reason::Io(io::ErrorKind::NotFound.into()))
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

    let options = ctx.filesystem_options();
    let resolved_location = resolve_link(current_directory, path, options)?;

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

    if let Err(reason) =
        options.run_custom_validation(&resolved_location, fragment)
    {
        log::debug!(
            "Custom validation reported \"{}\" as invalid because {}",
            resolved_location.display(),
            reason
        );
        return Err(reason);
    }

    Ok(())
}

/// Options to be used with [`resolve_link()`].
#[derive(Clone)]
#[cfg_attr(
    feature = "serde-1",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct Options {
    root_directory: Option<PathBuf>,
    default_file: OsString,
    links_may_traverse_the_root_directory: bool,
    follow_symlinks: bool,
    // Note: the key is normalised to lowercase to make sure extensions are
    // case insensitive
    alternate_extensions: HashMap<String, Vec<OsString>>,
    #[serde(skip, default = "nop_custom_validation")]
    custom_validation: Arc<dyn Fn(&Path, Option<&str>) -> Result<(), Reason>>,
}

impl Options {
    /// The name used by [`Options::default_file()`].
    pub const DEFAULT_FILE: &'static str = "index.html";

    /// A mapping of possible alternate extensions to try when checking a
    /// filesystem link.
    pub fn default_alternate_extensions(
    ) -> impl IntoIterator<Item = (OsString, impl IntoIterator<Item = OsString>)>
    {
        const MAPPING: &'static [(&'static str, &'static [&'static str])] =
            &[("md", &["html"])];

        MAPPING.iter().map(|(ext, alts)| {
            (OsString::from(ext), alts.iter().map(OsString::from))
        })
    }

    /// Create a new [`Options`] populated with some sane defaults.
    pub fn new() -> Self {
        Options {
            root_directory: None,
            default_file: OsString::from(Options::DEFAULT_FILE),
            links_may_traverse_the_root_directory: false,
            follow_symlinks: true,
            alternate_extensions: Options::default_alternate_extensions()
                .into_iter()
                .map(|(key, values)| {
                    (
                        key.to_string_lossy().to_lowercase(),
                        values.into_iter().map(Into::into).collect(),
                    )
                })
                .collect(),
            custom_validation: nop_custom_validation(),
        }
    }

    /// Get the root directory, if one was provided.
    pub fn root_directory(&self) -> Option<&Path> {
        self.root_directory.as_ref().map(|p| &**p)
    }

    /// Set the [`Options::root_directory()`], automatically converting to its
    /// canonical form with [`dunce::canonicalize()`].
    pub fn with_root_directory<P: AsRef<Path>>(
        self,
        root_directory: P,
    ) -> io::Result<Self> {
        Ok(Options {
            root_directory: Some(dunce::canonicalize(root_directory)?),
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
    /// ([`Options::default_alternate_extensions()`]).
    pub fn alternate_extensions(
        &self,
    ) -> impl Iterator<Item = (&OsStr, impl Iterator<Item = &OsStr>)> {
        self.alternate_extensions.iter().map(|(key, value)| {
            (OsStr::new(key), value.iter().map(|alt| alt.as_os_str()))
        })
    }

    /// Set the [`Options::alternate_extensions()`] mapping.
    pub fn set_alternate_extensions<S, I, V>(mut self, alternates: I) -> Self
    where
        I: IntoIterator<Item = (S, V)>,
        S: Into<OsString>,
        V: IntoIterator<Item = S>,
    {
        self.alternate_extensions = alternates
            .into_iter()
            .map(|(key, values)| {
                (
                    key.into().to_string_lossy().to_lowercase(),
                    values.into_iter().map(Into::into).collect(),
                )
            })
            .collect();

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

    /// Set [`Options::follow_symlinks()`].
    pub fn set_follow_symlinks(
        self,
        value: bool,
    ) -> Self {
        Options {
            follow_symlinks: value,
            ..self
        }
    }

    /// Set a function which will be executed after a link is resolved, allowing
    /// you to apply custom business logic.
    pub fn set_custom_validation<F>(self, custom_validation: F) -> Self
    where
        F: Fn(&Path, Option<&str>) -> Result<(), Reason> + 'static,
    {
        let custom_validation = Arc::new(custom_validation);
        Options {
            custom_validation,
            ..self
        }
    }

    fn join(
        &self,
        current_dir: &Path,
        second: &Path,
    ) -> Result<PathBuf, Reason> {
        log::trace!(
            "Appending \"{}\" to \"{}\"",
            second.display(),
            current_dir.display()
        );

        if second.has_root() {
            // if the path is absolute (i.e. has a leading slash) then it's
            // meant to be relative to the root directory, not the current one
            match self.root_directory() {
                Some(root) => {
                    let mut buffer = root.to_path_buf();
                    // append everything except the bits that make it absolute
                    // (e.g. "/" or "C:\")
                    buffer.extend(remove_absolute_components(second));
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
                None => {
                    log::warn!("The bit to be appended is absolute, but we don't have a \"root\" directory to resolve relative to");
                    Err(Reason::TraversesParentDirectories)
                },
            }
        } else {
            Ok(current_dir.join(second))
        }
    }

    /// Gets the canonical version of a particular path, resolving symlinks and
    /// other filesystem quirks.
    ///
    /// This will fail if the item doesn't exist.
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, Reason> {
        let f = |p| match self.follow_symlinks {
            true => dunce::canonicalize(p),
            false => Ok(normalize_path(p)),
        };
        
        let mut canonical = f(path)?;

        if canonical.is_dir() {
            log::trace!(
                "Appending the default file name because \"{}\" is a directory",
                canonical.display()
            );

            canonical.push(&self.default_file);
            // we need to canonicalize again because the default file may be a
            // symlink, or not exist at all
            if self.follow_symlinks || !canonical.exists() {
                canonical = dunce::canonicalize(canonical)?;
            }
        }

        Ok(canonical)
    }

    fn sanity_check(&self, path: &Path) -> Result<(), Reason> {
        log::trace!("Applying sanity checks to \"{}\"", path.display());

        if let Some(root) = self.root_directory() {
            log::trace!(
                "Checking if \"{}\" is allowed to leave \"{}\"",
                path.display(),
                root.display()
            );

            if !(self.links_may_traverse_the_root_directory
                || path.starts_with(root))
            {
                log::trace!(
                    "\"{}\" traverses outside the \"root\" directory",
                    path.display()
                );
                return Err(Reason::TraversesParentDirectories);
            }
        }

        Ok(())
    }

    /// sometimes the file being linked to may be usable with another extension
    /// (e.g. in mdbook, markdown files can be linked to with the HTML
    /// extension).
    fn possible_names(
        &self,
        original: PathBuf,
    ) -> impl IntoIterator<Item = PathBuf> {
        let mut names = vec![original.clone()];

        if let Some(alternatives) = original
            .extension()
            .map(|ext| ext.to_string_lossy().to_lowercase())
            .and_then(|ext| self.alternate_extensions.get(&ext))
        {
            for alternative in alternatives {
                names.push(original.with_extension(alternative));
            }
        }

        log::trace!(
            "Possible candidates for \"{}\" are {:?}",
            original.display(),
            names
        );

        names
    }

    fn run_custom_validation(
        &self,
        resolved_path: &Path,
        fragment: Option<&str>,
    ) -> Result<(), Reason> {
        (self.custom_validation)(resolved_path, fragment)
    }
}

fn nop_custom_validation(
) -> Arc<dyn Fn(&Path, Option<&str>) -> Result<(), Reason>> {
    Arc::new(|_, _| Ok(()))
}

impl Default for Options {
    fn default() -> Self { Options::new() }
}

impl Debug for Options {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let Options {
            root_directory,
            default_file,
            links_may_traverse_the_root_directory,
            follow_symlinks,
            alternate_extensions,
            custom_validation: _,
        } = self;

        f.debug_struct("Options")
            .field("root_directory", root_directory)
            .field("default_file", default_file)
            .field(
                "links_may_traverse_the_root_directory",
                links_may_traverse_the_root_directory,
            )
            .field( "follow_symlinks", follow_symlinks)
            .field("alternate_extensions", alternate_extensions)
            .finish()
    }
}

impl PartialEq for Options {
    fn eq(&self, other: &Options) -> bool {
        let Options {
            root_directory,
            default_file,
            links_may_traverse_the_root_directory,
            follow_symlinks,
            alternate_extensions,
            custom_validation: _,
        } = self;

        root_directory == &other.root_directory
            && default_file == &other.default_file
            && links_may_traverse_the_root_directory
                == &other.links_may_traverse_the_root_directory
            && follow_symlinks == &other.follow_symlinks
            && alternate_extensions == &other.alternate_extensions
    }
}

fn remove_absolute_components(
    path: &Path,
) -> impl Iterator<Item = Component> + '_ {
    path.components()
        .skip_while(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BasicContext;
    use std::sync::atomic::{AtomicBool, Ordering};

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

    fn init_logging() {
        let _ = env_logger::builder()
            .filter(Some("linkcheck"), log::LevelFilter::Trace)
            .is_test(true)
            .try_init();
    }

    #[test]
    fn resolve_mod_relative_to_validation_dir() {
        init_logging();
        let current_dir = validation_dir();
        let link = "mod.rs";
        let options = Options::default();

        let got =
            resolve_link(&current_dir, Path::new(link), &options).unwrap();

        assert_eq!(got, current_dir.join(link));
    }

    #[test]
    fn custom_validation_function_gets_called() {
        init_logging();
        let current_dir = validation_dir();
        let link = "mod.rs";
        let called = Arc::new(AtomicBool::new(false));
        let called_2 = Arc::clone(&called);
        let mut ctx = BasicContext::default();
        ctx.options = Options::default().set_custom_validation(move |_, _| {
            called_2.store(true, Ordering::SeqCst);
            Ok(())
        });

        check_filesystem(&current_dir, Path::new(link), None, &ctx).unwrap();

        assert!(called.load(Ordering::SeqCst))
    }

    #[test]
    fn detect_possible_directory_traversal_attacks() {
        init_logging();
        let temp = tempfile::tempdir().unwrap();
        let temp = dunce::canonicalize(temp.path()).unwrap();
        let foo = temp.join("foo");
        let bar = foo.join("bar");
        let baz = bar.join("baz");
        let options = Options::default().with_root_directory(&temp).unwrap();
        touch(&options.default_file, &[&temp, &foo, &bar, &baz]);
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
            temp.join(&options.default_file)
        );
        // but a directory traversal attack isn't
        let bad_path = if cfg!(windows) {
            "../../../../../../../../../../../../../../../../../Windows/System32/cmd.exe"
        } else {
            "../../../../../../../../../../../../../../../../../etc/passwd"
        };
        let traverses_parent_dir = resolve(bad_path).unwrap_err();
        assert!(
            matches!(traverses_parent_dir, Reason::TraversesParentDirectories),
            "{:?} should have traversed the parent directory",
            traverses_parent_dir
        );
    }

    #[test]
    fn links_with_a_leading_slash_are_relative_to_the_root() {
        init_logging();
        let temp = tempfile::tempdir().unwrap();
        let temp = dunce::canonicalize(temp.path()).unwrap();
        let foo = temp.join("foo");
        let bar = temp.join("bar");
        let options = Options::default().with_root_directory(&temp).unwrap();
        touch(&options.default_file, &[&temp, &foo, &bar]);
        let link = Path::new("/bar");

        let got = resolve_link(&foo, link, &options).unwrap();

        assert_eq!(got, bar.join(&options.default_file));
    }

    #[test]
    fn link_to_a_file_we_know_doesnt_exist() {
        init_logging();
        let temp = tempfile::tempdir().unwrap();
        let temp = dunce::canonicalize(temp.path()).unwrap();
        let options = Options::default().with_root_directory(&temp).unwrap();
        let link = Path::new("./bar");

        let err = resolve_link(&temp, link, &options).unwrap_err();

        assert!(err.file_not_found());
    }

    #[test]
    fn absolute_link_with_no_root_set_is_an_error() {
        init_logging();
        let temp = tempfile::tempdir().unwrap();
        let temp = dunce::canonicalize(temp.path()).unwrap();
        let options = Options::default();
        let link = Path::new("/bar");

        let err = resolve_link(&temp, link, &options).unwrap_err();

        assert!(matches!(err, Reason::TraversesParentDirectories));
    }

    #[test]
    fn a_link_that_is_allowed_to_traverse_the_root_dir() {
        init_logging();
        let temp = tempfile::tempdir().unwrap();
        let temp = dunce::canonicalize(temp.path()).unwrap();
        let foo = temp.join("foo");
        let bar = temp.join("bar");
        touch(Options::DEFAULT_FILE, &[&temp, &foo, &bar]);
        let options = Options::default()
            .with_root_directory(&foo)
            .unwrap()
            .set_links_may_traverse_the_root_directory(true);
        let link = Path::new("../bar/index.html");

        let got = resolve_link(&foo, link, &options).unwrap();

        assert_eq!(got, bar.join("index.html"));
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_from_root_tree_outside_is_not_resolved() {
        use std::os::unix::fs;

        init_logging();
        let temp = tempfile::tempdir().unwrap();
        let temp = dunce::canonicalize(temp.path()).unwrap();
        let foo = temp.join("foo");
        let bar = temp.join("bar");
        touch(Options::DEFAULT_FILE, &[&temp, &foo]);
        touch(Options::DEFAULT_FILE, &[&temp, &bar]);
        fs::symlink("../bar/index.html",foo.join("link.html").as_path()).unwrap();
        let options = Options::default()
            .with_root_directory(&foo)
            .unwrap()
            .set_links_may_traverse_the_root_directory(false)
            .set_follow_symlinks(false);
        let link = Path::new("link.html");

        let got = resolve_link(&foo, link, &options).unwrap();

        assert_eq!(got, foo.join("link.html"));
    }

    #[test]
    fn markdown_files_can_be_used_as_html() {
        init_logging();
        let temp = tempfile::tempdir().unwrap();
        let temp = dunce::canonicalize(temp.path()).unwrap();
        touch("index.html", &[&temp]);
        let link = "index.md";
        // let options = Options::default()
        //     .set_alternate_extensions(Options::DEFAULT_ALTERNATE_EXTENSIONS);
        let options = Options::default()
            .set_alternate_extensions(Options::default_alternate_extensions());

        let got = resolve_link(&temp, Path::new(link), &options).unwrap();

        assert_eq!(got, temp.join("index.html"));
    }

    #[test]
    fn join_paths() {
        init_logging();
        let temp = tempfile::tempdir().unwrap();
        let temp = dunce::canonicalize(temp.path()).unwrap();
        let foo = temp.join("foo");
        let bar = foo.join("bar");
        let baz = bar.join("baz");
        let baz_index = baz.join("index.html");
        touch("index.html", &[&temp, &foo, &bar, &baz]);
        let options = Options::default().with_root_directory(&temp).unwrap();

        let inputs = vec![
            ("/foo", &temp, &foo),
            ("foo", &temp, &foo),
            ("foo/bar", &temp, &bar),
            ("foo/bar/baz", &temp, &baz),
            ("/foo/bar/baz/index.html", &temp, &baz_index),
            ("bar/baz", &foo, &baz),
            ("baz", &bar, &baz),
            ("index.html", &baz, &baz_index),
        ];

        for (link, base, should_be) in inputs {
            let got = options.join(base, Path::new(link)).unwrap();
            assert_eq!(got, *should_be);
        }
    }
}
