//! Node.js runtime — calls `node.exe` and `npm` directly via the path
//! configured in loom.toml (`node.path`). No version manager (mise, nvm,
//! fnm, ...) is required. If `path` is unset, loom falls back to whatever
//! `node` / `npm` is on `PATH`.

use super::{Lang, Runtime};
use crate::config::Config;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct NodeRuntime<'a> {
    cfg: &'a Config,
}

impl<'a> NodeRuntime<'a> {
    pub fn new(cfg: &'a Config) -> Self {
        Self { cfg }
    }

    /// Resolve the directory that contains `node.exe` and friends. If a
    /// `node.path` is configured, that's the source of truth. Otherwise we
    /// fall through to `PATH` lookup.
    fn node_dir(&self) -> Option<PathBuf> {
        if let Some(p) = &self.cfg.node.path {
            return p.parent().map(|d| d.to_path_buf());
        }
        which_dir("node")
    }

    /// Ensure the project directory (e.g. `C:\Loom\loom\nodeapp`) exists.
    /// Windows' `CreateProcessW` returns `ERROR_DIRECTORY` (os error 267,
    /// "目录名称无效") if `lpCurrentDirectory` points at a non-existent
    /// path, which is exactly what we pass via `Command::current_dir`.
    /// `create_dir_all` is a no-op when the directory already exists.
    fn ensure_project(&self) -> Result<()> {
        crate::config::ensure_dir(&self.cfg.node_project())
    }

    /// Pick the right `npm` executable for the configured node.
    fn npm_cmd(&self) -> Result<Command> {
        self.ensure_project()?;
        let mut cmd = if let Some(dir) = self.node_dir() {
            #[cfg(windows)]
            let candidates = ["npm.cmd", "npm.exe", "npm"];
            #[cfg(not(windows))]
            let candidates = ["npm"];
            let mut resolved = None;
            for c in candidates {
                let p = dir.join(c);
                if p.exists() {
                    resolved = Some(Command::new(p));
                    break;
                }
            }
            resolved.unwrap_or_else(|| Command::new("npm"))
        } else {
            Command::new("npm")
        };
        cmd.current_dir(self.cfg.node_project());
        self.apply_proxy(&mut cmd);
        Ok(cmd)
    }

    /// Same idea, for `npx` (used by shim dispatch).
    fn npx_cmd(&self) -> Result<Command> {
        self.ensure_project()?;
        let mut cmd = if let Some(dir) = self.node_dir() {
            #[cfg(windows)]
            let candidates = ["npx.cmd", "npx.exe", "npx"];
            #[cfg(not(windows))]
            let candidates = ["npx"];
            let mut resolved = None;
            for c in candidates {
                let p = dir.join(c);
                if p.exists() {
                    resolved = Some(Command::new(p));
                    break;
                }
            }
            resolved.unwrap_or_else(|| Command::new("npx"))
        } else {
            Command::new("npx")
        };
        cmd.current_dir(self.cfg.node_project());
        self.apply_proxy(&mut cmd);
        Ok(cmd)
    }

    fn apply_proxy(&self, cmd: &mut Command) {
        if let Some(ref url) = self.cfg.proxy_url {
            if !url.is_empty() {
                cmd.env("HTTP_PROXY", url);
                cmd.env("HTTPS_PROXY", url);
            }
        }
    }

    /// Run a configured command and stream its output. Errors on non-zero exit.
    fn run_status(&self, mut cmd: Command) -> Result<()> {
        let status = cmd
            .status()
            .with_context(|| format!("spawning {:?}", cmd.get_program()))?;
        if !status.success() {
            bail!(
                "command failed with exit code {:?}: {:?}",
                status.code(),
                cmd.get_args().collect::<Vec<_>>()
            );
        }
        Ok(())
    }
}

impl<'a> Runtime for NodeRuntime<'a> {
    fn cfg(&self) -> &Config {
        self.cfg
    }

    fn lang(&self) -> Lang {
        Lang::Node
    }

    fn project_dir(&self) -> PathBuf {
        self.cfg.node_project()
    }

    fn bin_dir(&self) -> PathBuf {
        self.cfg.node_project().join("node_modules").join(".bin")
    }

    fn shims_dir(&self) -> PathBuf {
        self.cfg.shims_dir()
    }

    fn install(&self, packages: &[String], _dev: bool, dry_run: bool) -> Result<()> {
        if packages.is_empty() {
            bail!("no packages specified");
        }
        let mut cmd = self.npm_cmd()?;
        cmd.arg("install");
        for p in packages {
            cmd.arg(p);
        }
        if dry_run {
            println!("[dry-run] {:?}", cmd);
            return Ok(());
        }
        self.run_status(cmd)?;
        // After a successful install, mirror any new binaries in
        // `node_modules/.bin/` as shims under `<project>/shims/`. The user
        // expects the new binary to be on PATH right away; otherwise they
        // have to run `loom node shim add <name>` separately.
        crate::shim::auto_shim_binaries(self)?;
        Ok(())
    }

    fn uninstall(&self, packages: &[String], dry_run: bool) -> Result<()> {
        if packages.is_empty() {
            bail!("no packages specified");
        }
        let mut cmd = self.npm_cmd()?;
        cmd.arg("uninstall");
        for p in packages {
            cmd.arg(p);
        }
        if dry_run {
            println!("[dry-run] {:?}", cmd);
            return Ok(());
        }
        self.run_status(cmd)?;
        // Sweep shims — anything whose binary has disappeared from
        // every runtime's bin directory is an orphan. Pass all bin
        // dirs so a Python-owned shim with the same name doesn't
        // get pulled down just because Node lost its copy.
        crate::shim::cleanup_orphan_shims(self, &self.cfg.all_bin_dirs())?;
        Ok(())
    }

    fn list(&self) -> Result<Vec<String>> {
        let bin = self.bin_dir();
        if !bin.exists() {
            return Ok(vec![]);
        }
        let mut out = Vec::new();
        for entry in
            std::fs::read_dir(&bin).with_context(|| format!("reading {}", bin.display()))?
        {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(stripped) = name.strip_suffix(".cmd") {
                out.push(stripped.to_string());
            } else if let Some(stripped) = name.strip_suffix(".ps1") {
                out.push(stripped.to_string());
            } else {
                out.push(name);
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }

    fn status(&self) -> Result<String> {
        let mut cmd = self.npm_cmd()?;
        cmd.arg("outdated");
        let output = cmd
            .output()
            .with_context(|| format!("spawning {:?}", cmd.get_program()))?;
        let mut s = String::from_utf8_lossy(&output.stdout).to_string();
        if !output.stderr.is_empty() {
            s.push_str(&String::from_utf8_lossy(&output.stderr));
        }
        Ok(s)
    }

    fn upgrade(&self, packages: &[String], force: bool, dry_run: bool) -> Result<()> {
        if packages.is_empty() {
            bail!("Usage: loom node upgrade name[@version]... [--force]");
        }
        let resolved: Vec<String> = packages
            .iter()
            .map(|p| {
                if p.starts_with("--") {
                    p.clone()
                } else if p.contains('@') {
                    p.clone()
                } else {
                    format!("{p}@latest")
                }
            })
            .collect();
        let mut cmd = self.npm_cmd()?;
        cmd.arg("install");
        for a in &resolved {
            cmd.arg(a);
        }
        if force {
            cmd.arg("--force");
        }
        if dry_run {
            println!("[dry-run] {:?}", cmd);
            return Ok(());
        }
        self.run_status(cmd)?;
        // Upgraded package may have introduced new binaries — shim them.
        crate::shim::auto_shim_binaries(self)?;
        Ok(())
    }

    fn run(&self, bin: &str, args: &[String]) -> Result<()> {
        // Use `npx` with the explicit binary path.
        let mut cmd = self.npx_cmd()?;
        let bin_path = self.bin_dir().join(bin);
        cmd.arg(&bin_path);
        for a in args {
            cmd.arg(a);
        }
        let status = cmd
            .status()
            .with_context(|| format!("spawning {:?}", cmd.get_program()))?;
        std::process::exit(status.code().unwrap_or(1));
    }

    fn rebuild(&self, dry_run: bool) -> Result<()> {
        let mut cmd = self.npm_cmd()?;
        cmd.arg("rebuild");
        if dry_run {
            println!("[dry-run] {:?}", cmd);
            return Ok(());
        }
        self.run_status(cmd)
    }
}

/// Find the directory containing `binary` by walking PATH. Returns the
/// first match, or `None` if not found.
fn which_dir(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for ext in ["", ".exe", ".cmd", ".bat"] {
            let candidate = dir.join(format!("{binary}{ext}"));
            if candidate.is_file() {
                return Some(dir);
            }
        }
    }
    None
}

#[allow(dead_code)]
fn _ensure_path_uses(_p: &Path) {} // reserved for future strict-mode checks
