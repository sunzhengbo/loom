//! Runtime abstraction.
//!
//! A `Runtime` represents one language ecosystem (Node, Python, ...) and the
//! commands we need to manage its project-local package set.

pub mod node;
pub mod python;

use anyhow::Result;
use std::path::PathBuf;

use crate::config::Config;

pub enum Lang {
    Node,
    Python,
}

impl Lang {
    pub fn as_str(&self) -> &'static str {
        match self {
            Lang::Node => "node",
            Lang::Python => "python",
        }
    }
}

/// Common operations every language runtime must support.
pub trait Runtime {
    /// Borrow the underlying config. Used by the shim subsystem which needs
    /// the global shims directory rather than any per-runtime path.
    fn cfg(&self) -> &Config;

    /// Language name: "node" or "python".
    fn lang(&self) -> Lang;

    /// Project root directory (e.g. `C:\Loom\nodeapp`).
    fn project_dir(&self) -> PathBuf;

    /// Directory containing the language's installed binaries.
    /// For Node this is `node_modules/.bin`. For Python this is the venv's
    /// `Scripts` (Windows) or `bin` (Unix) directory.
    fn bin_dir(&self) -> PathBuf;

    /// Per-runtime shim scripts directory. Both runtimes now share a
    /// single shim dir at the loom root; this method just returns
    /// that one path. Kept on the trait so the auto-shim and cleanup
    /// helpers can be written generically.
    fn shims_dir(&self) -> PathBuf;

    /// Install one or more packages.
    fn install(&self, packages: &[String], dev: bool, dry_run: bool) -> Result<()>;

    /// Uninstall one or more packages.
    fn uninstall(&self, packages: &[String], dry_run: bool) -> Result<()>;

    /// List installed binaries.
    fn list(&self) -> Result<Vec<String>>;

    /// Show outdated packages.
    fn status(&self) -> Result<String>;

    /// Upgrade packages.
    fn upgrade(&self, packages: &[String], force: bool, dry_run: bool) -> Result<()>;

    /// Run an arbitrary command in the project's runtime environment.
    /// Used by shims to invoke the actual binary.
    fn run(&self, bin: &str, args: &[String]) -> Result<()>;

    /// Rebuild native modules against the current runtime version.
    /// `Node` invokes `npm rebuild`; `Python` reinstalls all packages so
    /// that wheels are re-resolved against the new interpreter.
    fn rebuild(&self, dry_run: bool) -> Result<()>;

    /// Whether `auto_shim_binaries` should create a shim for a binary
    /// with the given (extension-stripped) name. The default is `true` —
    /// `Node`'s `node_modules/.bin/` only contains user-package binaries,
    /// so no filtering is needed. `Python` overrides this to filter out
    /// venv internals (`python`, `pip`, `activate`, ...) that live next
    /// to user-package entry points in `.venv/Scripts/`.
    fn should_auto_shim(&self, _stem: &str) -> bool {
        true
    }
}
