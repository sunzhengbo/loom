use crate::config::{Config, RootSource};
use anyhow::Result;
use colored::*;

pub fn run(cfg: &Config) -> Result<()> {
    println!("{}", "Loom configuration".bold());
    println!();
    println!("  {:<14} {}", "root".cyan(), root_label(cfg).bright_black());
    println!(
        "  {:<14} {}",
        "proxy".cyan(),
        cfg.proxy_url.as_deref().unwrap_or("(from environment)")
    );
    println!();
    println!("  {}", "node".green().bold());
    match &cfg.node.path {
        Some(p) => println!("    {:<14} {}", "path".cyan(), p.display()),
        None => println!(
            "    {:<14} {}",
            "path".cyan(),
            "<unset — using PATH>".yellow()
        ),
    }
    println!("    {:<14} {}", "project".cyan(), cfg.node.project_dir);
    println!();
    println!("  {}", "python".green().bold());
    match &cfg.python.path {
        Some(p) => println!("    {:<14} {}", "path".cyan(), p.display()),
        None => println!(
            "    {:<14} {}",
            "path".cyan(),
            "<unset — using PATH>".yellow()
        ),
    }
    println!("    {:<14} {}", "project".cyan(), cfg.python.project_dir);
    println!();
    println!("  {}", "shims".green().bold());
    if cfg.shims.dir.is_empty() {
        println!(
            "    {:<14} {} (loom root, alongside loom.exe)",
            "dir".cyan(),
            "<loom root>".bright_black()
        );
    } else {
        println!("    {:<14} {}", "dir".cyan(), cfg.shims.dir);
    }
    println!(
        "    {:<14} {}",
        "resolved".cyan(),
        cfg.shims_dir().display()
    );
    Ok(())
}

/// Show a non-leaking label for the root, e.g. "(from loom.exe directory)".
/// We deliberately don't print the resolved absolute path here — the user
/// can see it by re-running `loom info` after `loom config set root`
/// if they really want it. This keeps the output portable when the config is
/// shared.
fn root_label(cfg: &Config) -> String {
    match cfg.root_source {
        RootSource::Toml => "(from loom.toml)".to_string(),
        RootSource::LoomDir => "(from $LOOM_DIR)".to_string(),
        RootSource::Exe => "(from loom.exe directory)".to_string(),
    }
}
