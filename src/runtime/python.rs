//! Python runtime — calls `python.exe` directly via the path configured in
//! loom.toml (`python.path`). Uses the standard library's `venv` and
//! `pip` (no uv, no pyenv, no mise).
//!
//! The first install creates `.venv` (or whatever `python.venv` names it)
//! using `<python.path> -m venv`. Subsequent operations target that venv
//! via `<python.path> -m pip` so we don't depend on `pip` being on PATH.

use super::{Lang, Runtime};
use crate::config::Config;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;

pub struct PythonRuntime<'a> {
    cfg: &'a Config,
}

impl<'a> PythonRuntime<'a> {
    pub fn new(cfg: &'a Config) -> Self {
        Self { cfg }
    }

    fn venv_path(&self) -> PathBuf {
        self.cfg.python_venv_path()
    }

    /// Path to the python executable to use for venv creation / rebuild.
    /// Resolves `python.path` from config, falling back to PATH lookup.
    fn base_python(&self) -> Result<PathBuf> {
        if let Some(p) = &self.cfg.python.path {
            if p.is_file() {
                return Ok(p.clone());
            }
            bail!(
                "python.path = {} does not point to a file.\n  hint: run `loom config set python.path \"C:\\Path\\to\\python.exe\"`",
                p.display()
            );
        }
        // Fallback: walk PATH ourselves (avoids relying on `which` crate's
        // Windows quirks).
        let path = std::env::var_os("PATH").ok_or_else(|| anyhow::anyhow!("PATH is not set"))?;
        for dir in std::env::split_paths(&path) {
            for name in ["python.exe", "python"] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
        bail!(
            "no python executable found.\n  \
             hint: install Python or set `python.path` in loom.toml"
        )
    }

    /// venv's python executable (Windows: `Scripts\python.exe`).
    fn venv_python(&self) -> PathBuf {
        #[cfg(windows)]
        {
            self.venv_path().join("Scripts").join("python.exe")
        }
        #[cfg(not(windows))]
        {
            self.venv_path().join("bin").join("python")
        }
    }

    /// Ensure the project directory (e.g. `C:\Loom\loom\pythonapp`) exists.
    /// Same reason as the Node side: `Command::current_dir` will pass this
    /// path to `CreateProcessW` as `lpCurrentDirectory`, which fails with
    /// `ERROR_DIRECTORY` (267) on a non-existent path.
    fn ensure_project(&self) -> Result<()> {
        crate::config::ensure_dir(&self.cfg.python_project())
    }

    /// Create the venv using the configured python.
    fn ensure_venv(&self, dry_run: bool) -> Result<()> {
        if self.venv_path().exists() {
            return Ok(());
        }
        self.ensure_project()?;
        let py = self.base_python()?;
        if dry_run {
            println!(
                "[dry-run] {} -m venv {}",
                py.display(),
                self.venv_path().display()
            );
            return Ok(());
        }
        let mut cmd = Command::new(&py);
        cmd.arg("-m").arg("venv").arg(&self.venv_path());
        self.apply_proxy(&mut cmd);
        let status = cmd
            .status()
            .with_context(|| format!("spawning {}", py.display()))?;
        if !status.success() {
            bail!("venv creation failed (exit {:?})", status.code());
        }
        Ok(())
    }

    /// Run `<venv_python> -m pip <args...>`.
    fn pip(&self) -> Result<Command> {
        if !self.venv_path().exists() {
            bail!(
                "venv not initialized at {} — run `loom python install <pkg>` first",
                self.venv_path().display()
            );
        }
        self.ensure_project()?;
        let py = self.venv_python();
        let mut cmd = Command::new(&py);
        cmd.arg("-m").arg("pip");
        cmd.current_dir(self.cfg.python_project());
        self.apply_proxy(&mut cmd);
        Ok(cmd)
    }

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

    fn apply_proxy(&self, cmd: &mut Command) {
        if let Some(ref url) = self.cfg.proxy_url {
            if !url.is_empty() {
                cmd.env("HTTP_PROXY", url);
                cmd.env("HTTPS_PROXY", url);
            }
        }
    }
}

impl<'a> Runtime for PythonRuntime<'a> {
    fn cfg(&self) -> &Config {
        self.cfg
    }

    fn lang(&self) -> Lang {
        Lang::Python
    }

    fn project_dir(&self) -> PathBuf {
        self.cfg.python_project()
    }

    fn bin_dir(&self) -> PathBuf {
        #[cfg(windows)]
        {
            self.venv_path().join("Scripts")
        }
        #[cfg(not(windows))]
        {
            self.venv_path().join("bin")
        }
    }

    fn shims_dir(&self) -> PathBuf {
        self.cfg.shims_dir()
    }

    fn install(&self, packages: &[String], _dev: bool, dry_run: bool) -> Result<()> {
        if packages.is_empty() {
            bail!("no packages specified");
        }
        self.ensure_venv(dry_run)?;
        let mut cmd = self.pip()?;
        cmd.arg("install");
        for p in packages {
            cmd.arg(p);
        }
        if dry_run {
            println!("[dry-run] {:?}", cmd);
            return Ok(());
        }
        self.run_status(cmd)?;
        // Mirror any new entry points in `.venv/Scripts/` as shims. The
        // entry point name comes from each package's setup.cfg / pyproject
        // — pip doesn't tell us directly, so we just observe the venv.
        crate::shim::auto_shim_binaries(self)?;
        Ok(())
    }

    fn uninstall(&self, packages: &[String], dry_run: bool) -> Result<()> {
        if packages.is_empty() {
            bail!("no packages specified");
        }
        if !self.venv_path().exists() {
            bail!("venv not initialized at {}", self.venv_path().display());
        }
        let mut cmd = self.pip()?;
        cmd.arg("uninstall").arg("-y");
        for p in packages {
            cmd.arg(p);
        }
        if dry_run {
            println!("[dry-run] {:?}", cmd);
            return Ok(());
        }
        self.run_status(cmd)?;
        // Sweep shims — anything whose entry point has disappeared
        // from every runtime's bin directory is an orphan. Pass all
        // bin dirs so a Node-owned shim with the same name doesn't
        // get pulled down just because Python lost its copy.
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
            if !entry.path().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let stem = name
                .strip_suffix(".cmd")
                .or_else(|| name.strip_suffix(".ps1"))
                .or_else(|| name.strip_suffix(".bat"))
                .or_else(|| name.strip_suffix(".exe"))
                .unwrap_or(&name)
                .to_string();
            if stem.is_empty() {
                continue;
            }
            #[cfg(windows)]
            if stem == name {
                continue;
            }
            if self.should_auto_shim(&stem) {
                out.push(stem);
            }
        }
        out.sort();
        out.dedup();
        Ok(out)
    }

    fn status(&self) -> Result<String> {
        if !self.venv_path().exists() {
            return Ok(format!(
                "venv not initialized: {}",
                self.venv_path().display()
            ));
        }
        let mut cmd = self.pip()?;
        cmd.arg("list").arg("--outdated");
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
        self.ensure_venv(dry_run)?;
        let mut cmd = self.pip()?;
        if packages.is_empty() {
            cmd.arg("install").arg("--upgrade");
        } else {
            cmd.arg("install").arg("--upgrade");
            for p in packages {
                if p.starts_with("--") {
                    cmd.arg(p);
                } else {
                    cmd.arg(p);
                }
            }
        }
        if force {
            cmd.arg("--force-reinstall");
        }
        if dry_run {
            println!("[dry-run] {:?}", cmd);
            return Ok(());
        }
        self.run_status(cmd)?;
        // Upgraded package may have added/replaced entry points — shim them.
        crate::shim::auto_shim_binaries(self)?;
        Ok(())
    }

    fn run(&self, bin: &str, args: &[String]) -> Result<()> {
        if !self.venv_path().exists() {
            bail!(
                "venv not initialized at {} — run `loom python install <pkg>` first",
                self.venv_path().display()
            );
        }
        #[cfg(windows)]
        let bin_path = self.bin_dir().join(format!("{bin}.exe"));
        #[cfg(not(windows))]
        let bin_path = self.bin_dir().join(bin);

        let mut cmd = Command::new(&bin_path);
        for a in args {
            cmd.arg(a);
        }
        // venv scripts need venv's bin on PATH so they find their companions.
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = std::env::split_paths(&path).collect::<Vec<_>>();
        paths.insert(0, self.bin_dir());
        cmd.env("PATH", std::env::join_paths(paths)?);
        cmd.env("VIRTUAL_ENV", &self.venv_path());
        self.apply_proxy(&mut cmd);

        let status = cmd
            .status()
            .with_context(|| format!("spawning {}", bin_path.display()))?;
        std::process::exit(status.code().unwrap_or(1));
    }

    fn rebuild(&self, dry_run: bool) -> Result<()> {
        if !self.venv_path().exists() {
            bail!(
                "venv not initialized at {} — nothing to rebuild",
                self.venv_path().display()
            );
        }
        // 1. Freeze current packages.
        let mut freeze_cmd = self.pip()?;
        freeze_cmd.arg("freeze");
        let freeze_output = freeze_cmd
            .output()
            .with_context(|| format!("spawning {:?}", freeze_cmd.get_program()))?;
        if !freeze_output.status.success() {
            bail!(
                "pip freeze failed: {}",
                String::from_utf8_lossy(&freeze_output.stderr)
            );
        }
        let pkgs: Vec<String> = String::from_utf8_lossy(&freeze_output.stdout)
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.split("==").next().unwrap_or(l).to_string())
            .collect();

        if pkgs.is_empty() {
            println!("nothing to rebuild (no packages installed)");
            return Ok(());
        }

        // 2. Reinstall them all against the (possibly new) interpreter.
        let mut cmd = self.pip()?;
        cmd.arg("install").arg("--force-reinstall");
        for p in &pkgs {
            cmd.arg(p);
        }
        if dry_run {
            println!("[dry-run] would reinstall {} packages:", pkgs.len());
            for p in &pkgs {
                println!("  {p}");
            }
            return Ok(());
        }
        println!(
            "reinstalling {} packages against current Python…",
            pkgs.len()
        );
        self.run_status(cmd)
    }

    fn should_auto_shim(&self, stem: &str) -> bool {
        // `.venv/Scripts/` mingles the venv's own utilities with the
        // user-package entry points. Filter out the former so we only
        // shim real CLIs the user installed. Comparison is
        // case-insensitive — Windows filenames are.
        let lower = stem.to_lowercase();
        // Strip trailing `.exe` if the runtime gave us the full filename
        // (defensive — auto_shim_binaries already strips it, but cheap
        // to be explicit).
        let lower = lower.strip_suffix(".exe").unwrap_or(&lower).to_string();
        // Interpreter and core venv tools
        if lower == "python"
            || lower == "pythonw"
            || lower == "python3"
            || lower.starts_with("python3.")
        {
            return false;
        }
        if lower == "pip" || lower == "pip3" || lower.starts_with("pip3.") {
            return false;
        }
        // Activation helpers — not real executables
        if lower == "activate" || lower == "deactivate" {
            return false;
        }
        // setuptools / wheel entry points
        if lower == "easy_install" || lower == "wheel" {
            return false;
        }
        true
    }
}
