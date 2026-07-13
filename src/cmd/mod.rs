//! Subcommand implementations.
//!
//! Each subcommand is a tiny module that takes a `&dyn Runtime` and the
//! relevant arguments, then dispatches to the runtime.

pub mod config;
pub mod info;
pub mod install;
pub mod list;
pub mod rebuild;
pub mod shim;
pub mod status;
pub mod uninstall;
pub mod upgrade;
