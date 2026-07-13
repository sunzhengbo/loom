//! Dynamic shim management.
//!
//! A "shim" is a hard link to `loom.exe` placed in the single shared
//! `<root>/<shims.dir>/` directory. When the user runs `codex` on PATH,
//! Windows launches the shim, which is literally `loom.exe` — `loom`
//! detects it was invoked through a shim by comparing `current_exe().parent()`
//! against `cfg.shims_dir()`. The runtime is then decided at dispatch time
//! by looking up the binary in each runtime's bin directory (Node first,
//! Python second).

use crate::config::Config;
use crate::runtime::Runtime;
use anyhow::{bail, Context, Result};
use colored::*;
use std::path::PathBuf;
use std::process::Command;

pub fn add(rt: &dyn Runtime, name: &str) -> Result<()> {
    let shims = rt.shims_dir();
    crate::config::ensure_dir(&shims)?;

    let target = shims.join(shim_filename(name));
    if target.exists() || target.symlink_metadata().is_ok() {
        bail!("shim already exists: {}", target.display());
    }

    // Verify the binary actually exists in the runtime's bin dir.
    let bin = rt.bin_dir().join(name);
    let bin_exe = rt.bin_dir().join(format!("{name}.exe"));
    let bin_cmd = rt.bin_dir().join(format!("{name}.cmd"));
    if !(bin.is_file() || bin_exe.is_file() || bin_cmd.is_file()) {
        bail!(
            "binary `{}` not found for {} runtime\n  hint: run `loom {} install {}` first",
            name,
            rt.lang().as_str(),
            rt.lang().as_str(),
            name
        );
    }

    let source = std::env::current_exe().context("locating loom.exe")?;
    create_hard_link(&source, &target)
        .with_context(|| format!("creating shim hard link: {}", target.display()))?;
    println!(
        "{} shim created: {}",
        "ok".green().bold(),
        target.display()
    );
    Ok(())
}

pub fn remove(cfg: &Config, name: &str) -> Result<()> {
    // All shims share one directory now. Look for the shim by name and
    // remove it; missing → bail with a clear message.
    let dir = cfg.shims_dir();
    let p = dir.join(shim_filename(name));
    if !p.exists() {
        bail!("no shim named `{}` found in {}", name, dir.display());
    }
    std::fs::remove_file(&p)
        .with_context(|| format!("removing shim: {}", p.display()))?;
    println!("{} shim removed: {}", "ok".yellow().bold(), p.display());
    Ok(())
}

pub fn list(cfg: &Config) -> Result<()> {
    // All shims share the loom root by default, so the listing also
    // sees loom.exe itself — skip it (it's the manager, not a shim).
    let dir = cfg.shims_dir();
    let mut any = false;
    if !dir.exists() {
        println!("(no shims)");
        return Ok(());
    }
    for entry in std::fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
    {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "loom.exe" {
            continue;
        }
        if is_shim_file(&name) {
            if !any {
                any = true;
            }
            println!(
                "  {}  {}",
                "shim".cyan(),
                name.trim_end_matches(shim_suffix()).cyan()
            );
        }
    }
    if !any {
        println!("(no shims)");
    }
    Ok(())
}

#[cfg(windows)]
pub fn shim_filename(name: &str) -> String {
    let stem = name.trim_end_matches(".exe");
    format!("{stem}.exe")
}

#[cfg(not(windows))]
pub fn shim_filename(name: &str) -> String {
    name.to_string()
}

/// After a successful package install, scan the runtime's bin directory
/// (e.g. `node_modules/.bin/` for Node, `.venv/Scripts/` for Python) and
/// create a shim for any binary that doesn't have one yet. This is the
/// common case — the user runs `loom <lang> install <pkg>` and immediately
/// wants the new binary on PATH.
///
/// Skips silently if a shim already exists for that name. Failures
/// (e.g. `mklink` not permitted) are surfaced as warnings, not errors —
/// the install itself already succeeded, the shim is a convenience.
pub fn auto_shim_binaries(rt: &dyn Runtime) -> Result<()> {
    let bin_dir = rt.bin_dir();
    if !bin_dir.exists() {
        return Ok(());
    }
    let shims_dir = rt.shims_dir();
    crate::config::ensure_dir(&shims_dir)?;

    let source = std::env::current_exe().context("locating loom.exe")?;
    let entries: Vec<_> = std::fs::read_dir(&bin_dir)
        .with_context(|| format!("reading {}", bin_dir.display()))?
        .filter_map(|e| e.ok())
        .collect();
    let mut created = 0usize;
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        // .bin/ on Windows has `<bin>`, `<bin>.cmd`, `<bin>.ps1` (and
        // sometimes `<bin>.exe`). They all represent the same binary;
        // dedupe by the bare stem.
        let stem = name
            .strip_suffix(".cmd")
            .or_else(|| name.strip_suffix(".ps1"))
            .or_else(|| name.strip_suffix(".exe"))
            .unwrap_or(&name)
            .to_string();
        if stem.is_empty() || stem == name {
            continue;
        }
        if !rt.should_auto_shim(&stem) {
            continue;
        }
        let shim_target = shims_dir.join(shim_filename(&stem));
        if shim_target.exists() {
            continue;
        }
        match create_hard_link(&source, &shim_target) {
            Ok(()) => {
                println!(
                    "{} shim created: {}",
                    "ok".green().bold(),
                    shim_target.display()
                );
                created += 1;
            }
            Err(e) => {
                eprintln!(
                    "{} could not create shim for `{}`: {}",
                    "warn".yellow(),
                    stem,
                    e
                );
            }
        }
    }
    if created > 0 {
        println!(
            "{} {} shim(s) ready — add `{}` to PATH to use them",
            "→".cyan(),
            created,
            shims_dir.display()
        );
    }
    Ok(())
}

/// After a successful package uninstall, remove any shim whose
/// corresponding binary is no longer in **any** runtime's bin
/// directory. This is the symmetric counterpart to
/// `auto_shim_binaries` — keep the two in lockstep so uninstalled
/// packages don't leave dangling shims that point at nothing.
///
/// `should_auto_shim` (from the runtime that's doing the uninstall)
/// gates both directions: a shim that was never created by us (e.g.
/// a manual `shim add` for a venv internal, or a shim that was
/// pinned by the other runtime) is never removed here.
///
/// `bin_dirs` should list every bin directory loom knows about
/// (typically `<node>/.bin` and `<venv>/Scripts`). A shim survives
/// as long as at least one of these still has a binary with the
/// same name — so uninstalling `black` from Python doesn't take
/// down the `opencode` shim that came from Node, and vice versa.
pub fn cleanup_orphan_shims(rt: &dyn Runtime, bin_dirs: &[std::path::PathBuf]) -> Result<()> {
    let shims_dir = rt.shims_dir();
    if !shims_dir.exists() {
        return Ok(());
    }

    // Build the set of live binary stems (lowercased — Windows paths
    // are case-insensitive). Apply `should_auto_shim` per-runtime
    // (we only know the gating runtime here, but the same rule
    // fires regardless of which bin_dir the binary came from).
    let mut live: std::collections::HashSet<String> = std::collections::HashSet::new();
    for bin_dir in bin_dirs {
        if !bin_dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(bin_dir)
            .with_context(|| format!("reading {}", bin_dir.display()))?
        {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let stem = name
                .strip_suffix(".cmd")
                .or_else(|| name.strip_suffix(".ps1"))
                .or_else(|| name.strip_suffix(".exe"))
                .unwrap_or(&name)
                .to_string();
            if !stem.is_empty() && rt.should_auto_shim(&stem) {
                live.insert(stem.to_lowercase());
            }
        }
    }

    let mut removed = 0usize;
    for entry in std::fs::read_dir(&shims_dir)
        .with_context(|| format!("reading {}", shims_dir.display()))?
    {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        // Shims now live alongside loom.exe — the dir also contains
        // nodeapp/, pythonapp/, tools/, loom.toml, etc. Skip anything
        // that isn't a shim-shaped .exe file.
        if !is_shim_file(&name) || name == "loom.exe" {
            continue;
        }
        let stem = name
            .strip_suffix(".exe")
            .unwrap_or(&name)
            .to_string();
        if stem.is_empty() {
            continue;
        }
        if !rt.should_auto_shim(&stem) {
            // Never auto-shimmed → never auto-removed. Manual `shim add`
            // entries fall through here.
            continue;
        }
        if live.contains(&stem.to_lowercase()) {
            // At least one runtime still has this binary, keep the shim.
            continue;
        }
        match std::fs::remove_file(entry.path()) {
            Ok(()) => {
                println!(
                    "{} shim removed: {}",
                    "ok".yellow(),
                    entry.path().display()
                );
                removed += 1;
            }
            Err(e) => {
                eprintln!(
                    "{} could not remove shim `{}`: {}",
                    "warn".yellow(),
                    name,
                    e
                );
            }
        }
    }
    if removed > 0 {
        println!("{} {} orphan shim(s) removed", "→".cyan(), removed);
    }
    Ok(())
}

fn shim_suffix() -> &'static str {
    #[cfg(windows)]
    {
        ".exe"
    }
    #[cfg(not(windows))]
    {
        ""
    }
}

fn is_shim_file(name: &str) -> bool {
    #[cfg(windows)]
    {
        name.ends_with(".exe")
    }
    #[cfg(not(windows))]
    {
        !name.ends_with(".cmd") && !name.ends_with(".bat") && !name.ends_with(".ps1")
    }
}

/// Create a hard link from `target` to `source`. On Windows we shell out to
/// `mklink /H` (Rust's `std::fs::hard_link` doesn't use the right Win32
/// `CreateHardLinkW` semantics for our use case, and we want consistent
/// behavior with what the user gets from `cmd /C mklink`).
pub fn create_hard_link(source: &std::path::Path, target: &PathBuf) -> Result<()> {
    #[cfg(windows)]
    {
        let src = source.to_string_lossy().to_string();
        let dst = target.to_string_lossy().to_string();
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/H", &dst, &src])
            .status()
            .context("spawning mklink")?;
        if !status.success() {
            bail!("mklink failed (exit {status:?}) — need NTFS and write access");
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::hard_link(source, target)
            .with_context(|| format!("hard_link({}, {})", source.display(), target.display()))?;
    }
    Ok(())
}
