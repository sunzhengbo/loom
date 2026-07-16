use clap::{Parser, Subcommand};
use colored::*;

mod cmd;
mod config;
mod runtime;
mod shim;

use runtime::Runtime;

#[derive(Parser, Debug)]
#[command(
    name = "loom",
    version,
    about = "Loom — project-local toolchain manager (Node + Python)",
    long_about = "Loom manages per-project toolchains for Node.js and Python, like mise but project-scoped. \
                  Tools are installed under a project directory and exposed via shim scripts, \
                  so the global environment is never polluted."
)]
struct Cli {
    /// Override config file path
    #[arg(long, global = true, env = "LOOM_CONFIG")]
    config: Option<String>,

    /// Show what would be done without executing
    #[arg(long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Manage Node.js packages
    Node {
        #[command(subcommand)]
        cmd: NodeCmd,
    },
    /// Manage Python packages
    Python {
        #[command(subcommand)]
        cmd: PythonCmd,
    },
    /// Show resolved paths and config
    Info,
    /// Manage loom.toml configuration
    Config {
        #[command(subcommand)]
        cmd: cmd::config::ConfigCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum NodeCmd {
    /// Install one or more packages
    Install { packages: Vec<String> },
    /// Uninstall one or more packages
    Uninstall { packages: Vec<String> },
    /// List installed binaries
    List,
    /// Show outdated packages
    Status,
    /// Upgrade packages (add @latest if no version given)
    Upgrade {
        packages: Vec<String>,
        /// Force reinstall
        #[arg(long)]
        force: bool,
    },
    /// Rebuild native modules against the current Node version
    /// (run this after `loom config set node.version`)
    Rebuild,
    /// Manage shim scripts
    Shim {
        #[command(subcommand)]
        cmd: ShimCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum PythonCmd {
    /// Install one or more packages
    Install {
        packages: Vec<String>,
        /// Install as dev dependency
        #[arg(long)]
        dev: bool,
    },
    /// Uninstall one or more packages
    Uninstall { packages: Vec<String> },
    /// List installed binaries
    List,
    /// Show outdated packages
    Status,
    /// Upgrade packages
    Upgrade {
        packages: Vec<String>,
        /// Force reinstall
        #[arg(long)]
        force: bool,
    },
    /// Rebuild all packages against the current Python interpreter
    /// (run this after `loom config set python.version`)
    Rebuild,
    /// Manage shim scripts
    Shim {
        #[command(subcommand)]
        cmd: ShimCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum ShimCmd {
    /// Create a shim for a binary
    Add { name: String },
    /// Remove a shim
    Remove { name: String },
    /// List existing shims
    List,
}

/// What shim mode detected at startup, if any.
struct ShimInvocation {
    name: String,
    args: Vec<String>,
}

fn main() {
    let cfg = match config::Config::load(None) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} {:#}", "error:".red().bold(), e);
            std::process::exit(1);
        }
    };

    let shim_invocation = if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let canon =
                |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
            let name = exe.file_stem().and_then(|s| s.to_str()).map(String::from);
            let args: Vec<String> = std::env::args().skip(1).collect();
            // Shims and loom.exe share the same directory by default
            // (both live in <root>/), so the parent check alone is not
            // enough. The filename disambiguates: loom.exe is the
            // manager, anything else is a shim.
            let is_loom_itself = exe.file_stem().map(|s| s == "loom").unwrap_or(false);
            if canon(parent) == canon(&cfg.shims_dir()) && !is_loom_itself {
                name.map(|n| ShimInvocation { name: n, args })
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    if let Some(inv) = shim_invocation {
        if let Err(e) = run_shim(&cfg, inv) {
            eprintln!("{} {:#}", "error:".red().bold(), e);
            std::process::exit(1);
        }
        return;
    }

    // NORMAL CLI MODE.
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("{} {:#}", "error:".red().bold(), e);
        std::process::exit(1);
    }
}

fn run_shim(cfg: &config::Config, inv: ShimInvocation) -> anyhow::Result<()> {
    // Shims live in a single shared directory. To know which runtime
    // owns the shim, look in each runtime's bin directory in order:
    // Node first (more common), then Python. Whichever has the
    // binary gets the invocation. If neither does, the user removed
    // the underlying package — surface a clear error.
    let node_rt = runtime::node::NodeRuntime::new(cfg);
    if has_binary(node_rt.bin_dir(), &inv.name) {
        return node_rt.run(&inv.name, &inv.args);
    }
    let py_rt = runtime::python::PythonRuntime::new(cfg);
    if has_binary(py_rt.bin_dir(), &inv.name) {
        return py_rt.run(&inv.name, &inv.args);
    }
    anyhow::bail!(
        "no binary `{}` found in node_modules/.bin or .venv/Scripts — \
         did the underlying package get uninstalled?",
        inv.name
    )
}

/// True if `bin_dir/<name>` (with any of `.cmd`/`.ps1`/`.exe` suffix)
/// is a regular file. Used by the shim dispatcher to find which
/// runtime a shim name belongs to.
fn has_binary(bin_dir: std::path::PathBuf, name: &str) -> bool {
    if !bin_dir.exists() {
        return false;
    }
    for ext in ["", ".exe", ".cmd", ".ps1"] {
        if bin_dir.join(format!("{name}{ext}")).is_file() {
            return true;
        }
    }
    false
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let cfg = config::Config::load(cli.config.as_deref().map(std::path::Path::new))?;

    match cli.command {
        Command::Info => {
            cmd::info::run(&cfg)?;
        }
        Command::Config { cmd } => {
            cmd::config::run(cmd, cli.config.as_deref())?;
        }
        Command::Node { cmd } => {
            let rt = runtime::node::NodeRuntime::new(&cfg);
            match cmd {
                NodeCmd::Install { packages } => cmd::install::run(&rt, &packages, cli.dry_run)?,
                NodeCmd::Uninstall { packages } => {
                    cmd::uninstall::run(&rt, &packages, cli.dry_run)?
                }
                NodeCmd::List => cmd::list::run(&rt)?,
                NodeCmd::Status => cmd::status::run(&rt)?,
                NodeCmd::Upgrade { packages, force } => {
                    cmd::upgrade::run(&rt, &packages, force, cli.dry_run)?
                }
                NodeCmd::Rebuild => cmd::rebuild::run(&rt, cli.dry_run)?,
                NodeCmd::Shim { cmd } => match cmd {
                    ShimCmd::Add { name } => shim::add(&rt, &name)?,
                    ShimCmd::Remove { name } => shim::remove(&cfg, &name)?,
                    ShimCmd::List => shim::list(&cfg)?,
                },
            }
        }
        Command::Python { cmd } => {
            let rt = runtime::python::PythonRuntime::new(&cfg);
            match cmd {
                PythonCmd::Install { packages, dev } => {
                    cmd::install::run_py(&rt, &packages, dev, cli.dry_run)?
                }
                PythonCmd::Uninstall { packages } => {
                    cmd::uninstall::run_py(&rt, &packages, cli.dry_run)?
                }
                PythonCmd::List => cmd::list::run_py(&rt)?,
                PythonCmd::Status => cmd::status::run_py(&rt)?,
                PythonCmd::Upgrade { packages, force } => {
                    cmd::upgrade::run_py(&rt, &packages, force, cli.dry_run)?
                }
                PythonCmd::Rebuild => cmd::rebuild::run_py(&rt, cli.dry_run)?,
                PythonCmd::Shim { cmd } => match cmd {
                    ShimCmd::Add { name } => shim::add(&rt, &name)?,
                    ShimCmd::Remove { name } => shim::remove(&cfg, &name)?,
                    ShimCmd::List => shim::list(&cfg)?,
                },
            }
        }
    }

    Ok(())
}
