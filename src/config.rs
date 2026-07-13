//! Configuration for Loom.
//!
//! Configuration is read from `loom.toml` (next to `loom.exe` by default).
//! If the file is missing, defaults are used.
//!
//! ## Root resolution
//!
//! When the resolved `root` is needed, loom tries the following sources in
//! order. The first one that yields a path wins; later ones are ignored.
//!
//! 1. **`root` field in `loom.toml`** — the user explicitly locked the
//!    install to a specific directory. Even if the path doesn't exist on
//!    disk, this value is honored; subsequent commands will surface a
//!    clear error if the path is wrong.
//! 2. **`$LOOM_DIR` environment variable** — runtime override. If the
//!    path doesn't exist, loom prints a warning and falls through.
//! 3. **`loom.exe`'s parent directory** — self-contained install:
//!    move the binary (with its `loom.toml`) and the install follows.
//! 4. **`C:\Loom` on Windows / `~/Loom` elsewhere** — last-resort
//!    fallback for the "I don't have a config yet" bootstrap case.

use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Root of the Loom installation (parent of nodeapp/).
    /// If absent, loom falls back to `$LOOM_DIR` → loom.exe's
    /// directory → `C:\Loom`. If present, this value wins unconditionally
    /// — it's how you "lock" a config to a particular install.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<PathBuf>,

    /// Where the resolved root came from. Set by `Config::load` for use by
    /// `info` so the user can see "from where" without leaking the path.
    /// Not serialized — it's purely a runtime aid.
    #[serde(skip)]
    pub root_source: RootSource,

    /// HTTP/HTTPS proxy URL. `None` means: loom does not set proxy
    /// env vars itself — rely on the calling shell's `HTTP_PROXY` /
    /// `HTTPS_PROXY` (the standard behavior of every CLI tool).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,

    #[serde(default)]
    pub node: NodeConfig,

    #[serde(default)]
    pub python: PythonConfig,

    #[serde(default)]
    pub shims: ShimsConfig,
}

/// Which level of the root-resolution chain supplied the final `root`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootSource {
    /// `root` field was set explicitly in loom.toml.
    Toml,
    /// `$LOOM_DIR` environment variable.
    LoomDir,
    /// loom.exe's own directory (self-contained install / last-resort default).
    Exe,
}

impl Default for RootSource {
    fn default() -> Self {
        RootSource::Exe
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Absolute path to the `node.exe` (or `node` on Unix) executable.
    /// loom calls this directly — no version manager required.
    /// `None` means: probe `PATH` at startup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,

    /// Project subdirectory under `root` for Node packages.
    pub project_dir: String,
}

/// Shim directory config. Both runtimes' shims live in this one
/// directory — loom figures out the language at dispatch time by
/// looking for the binary in each runtime's bin directory (Node
/// first, then Python).
///
/// `dir` is the subdirectory under `root`. When empty (the default),
/// shims land directly in `<root>/` next to `loom.exe` itself —
/// that's the "one PATH entry, no extra dir" layout. Set it to a
/// non-empty value (e.g. `"shims"`) to keep shims in their own
/// subdirectory, the old way.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShimsConfig {
    /// Subdirectory under `root` for all shims. Empty (default) means
    /// shims live in `<root>/` alongside `loom.exe`. A non-empty
    /// value puts shims in `<root>/<dir>/`.
    #[serde(default)]
    pub dir: String,
}

impl Default for ShimsConfig {
    fn default() -> Self {
        Self {
            dir: String::new(),
        }
    }
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            path: None,
            project_dir: "nodeapp".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonConfig {
    /// Absolute path to the `python.exe` (or `python` on Unix) executable.
    /// loom uses this to create the project venv and to rebuild after
    /// a Python interpreter change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,

    /// Project subdirectory under `root` for Python packages.
    pub project_dir: String,

    /// Path to virtualenv (relative to project_dir). `None` means put the
    /// venv next to project_dir as `.venv`.
    pub venv: Option<String>,
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self {
            path: None,
            project_dir: "pythonapp".to_string(),
            venv: Some(".venv".to_string()),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root: None,
            root_source: RootSource::Exe,
            proxy_url: None,
            node: NodeConfig::default(),
            python: PythonConfig::default(),
            shims: ShimsConfig::default(),
        }
    }
}

impl Config {
    /// Load config from `path`, falling back to defaults for any missing field.
    /// `root` is resolved in priority order: TOML > $LOOM_DIR > loom.exe
    /// directory > C:\Loom fallback.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let cfg_path = match path {
            Some(p) => p.to_path_buf(),
            None => default_config_path(),
        };

        let mut cfg: Config = if !cfg_path.exists() {
            Config::default()
        } else {
            let raw = std::fs::read_to_string(&cfg_path)
                .with_context(|| format!("reading config file: {}", cfg_path.display()))?;
            toml::from_str(&raw)
                .with_context(|| format!("parsing config file: {}", cfg_path.display()))?
        };

        // Normalize BEFORE root resolution: an empty `root` string means
        // "no explicit root" (the user ran `config set root null` or wrote
        // `root = ""`), so the env / loom.exe / fallback chain kicks in.
        // Same convention for `python.venv` since bare `null` isn't valid
        // in every TOML parser.
        if cfg
            .root
            .as_ref()
            .map(|p| p.as_os_str().is_empty())
            .unwrap_or(false)
        {
            cfg.root = None;
        }
        if cfg.python.venv.as_deref() == Some("") {
            cfg.python.venv = None;
        }
        if cfg.proxy_url.as_deref() == Some("") || cfg.proxy_url.as_deref() == Some("null") {
            cfg.proxy_url = None;
        }

        // Root resolution. Priority (highest first):
        //   1. loom.toml has an explicit `root` field — lock to that.
        //   2. $LOOM_DIR env var — runtime override. Falls through with a
        //      warning if the path doesn't exist.
        //   3. loom.exe's own directory — the default for a self-contained
        //      install. loom.exe is always there (it must be, since
        //      we're running), so this branch always succeeds.
        if cfg.root.is_some() {
            cfg.root_source = RootSource::Toml;
        } else if let Ok(v) = std::env::var("LOOM_DIR") {
            let p = PathBuf::from(&v);
            if p.exists() {
                cfg.root = Some(p);
                cfg.root_source = RootSource::LoomDir;
            } else {
                eprintln!(
                    "{} LOOM_DIR={} does not exist, falling back",
                    "warning:".yellow(),
                    p.display()
                );
            }
        }
        if cfg.root.is_none() {
            // Prefer walking up to find loom.toml — this also works
            // when invoked through a shim (hard link), where the exe's
            // parent is the shim dir, not the loom root.
            if let Some(root) = find_loom_root() {
                cfg.root = Some(root);
                cfg.root_source = RootSource::Exe;
            } else if let Ok(exe) = std::env::current_exe() {
                // Last-resort: just use the exe's parent.
                if let Some(parent) = exe.parent() {
                    cfg.root = Some(parent.to_path_buf());
                    cfg.root_source = RootSource::Exe;
                }
            }
        }
        // If we got here without setting root, something is very wrong
        // (we can't even read our own exe path). Panic loudly so users
        // see it immediately rather than getting confusing downstream errors.
        if cfg.root.is_none() {
            anyhow::bail!(
                "could not determine loom root: current_exe() failed.\n  \
                 This usually means the loom.exe path is unreadable."
            );
        }

        Ok(cfg)
    }

    /// Resolved root (always Some — guaranteed by `load`).
    pub fn root(&self) -> &PathBuf {
        self.root.as_ref().expect("Config::load ensures root is set")
    }

    /// Absolute path to the Node project directory.
    pub fn node_project(&self) -> PathBuf {
        self.root().join(&self.node.project_dir)
    }

    /// Absolute path to the shared shims directory. Both Node and
    /// Python shims live here; the dispatcher figures out the
    /// language at runtime by looking in each runtime's bin_dir.
    /// When `shims.dir` is empty, this returns `<root>/` itself —
    /// shims sit next to `loom.exe` as hard links.
    pub fn shims_dir(&self) -> PathBuf {
        self.root().join(&self.shims.dir)
    }

    /// Absolute path to the Python project directory.
    pub fn python_project(&self) -> PathBuf {
        self.root().join(&self.python.project_dir)
    }

    /// Path to the Python virtualenv. Always returns a real path —
    /// defaults to `<python_project>/.venv` when the config doesn't
    /// override `python.venv`.
    pub fn python_venv_path(&self) -> PathBuf {
        let venv = self
            .python
            .venv
            .clone()
            .unwrap_or_else(|| ".venv".to_string());
        self.python_project().join(venv)
    }

    /// Every bin directory loom knows about — used by shim cleanup
    /// to decide if a shim is still "alive" (at least one runtime
    /// still has the binary).
    pub fn all_bin_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        // Node: <node_project>/node_modules/.bin
        dirs.push(self.node_project().join("node_modules").join(".bin"));
        // Python: <venv>/Scripts (Windows) or <venv>/bin (Unix)
        let py_venv = self.python_venv_path();
        #[cfg(windows)]
        dirs.push(py_venv.join("Scripts"));
        #[cfg(not(windows))]
        dirs.push(py_venv.join("bin"));
        dirs
    }
}

fn default_config_path() -> PathBuf {
    // loom.toml lives next to loom.exe. When invoked through a shim
    // (hard link to loom.exe), `current_exe().parent()` is the shim
    // directory, not the loom root. Walk up the tree until we find
    // loom.toml — that marks the real loom install.
    find_loom_root()
        .map(|r| r.join("loom.toml"))
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|e| e.parent().map(|p| p.to_path_buf()))
                .map(|p| p.join("loom.toml"))
        })
        .unwrap_or_else(|| PathBuf::from("loom.toml"))
}

/// Walk up from the current executable looking for a directory that
/// contains `loom.toml`. That directory is the loom install root.
///
/// Why this exists: when loom.exe is invoked through a hard-link shim
/// (e.g. `nodeapp/shims/opencode.exe`), `current_exe()` returns the
/// shim's path, not loom.exe's path. Naively taking
/// `current_exe().parent()` would treat the shim directory as the root,
/// doubling up the path (`<shims>/<shims>`). Walking up until we hit
/// `loom.toml` is the only reliable way to recover the real root
/// without resorting to Windows-specific APIs.
fn find_loom_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent()?;
    loop {
        if dir.join("loom.toml").is_file() {
            return Some(dir.to_path_buf());
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return None,
        }
    }
}

#[allow(dead_code)]
pub fn ensure_dir(p: &Path) -> Result<()> {
    if !p.exists() {
        std::fs::create_dir_all(p).with_context(|| format!("creating directory: {}", p.display()))?;
    }
    Ok(())
}
