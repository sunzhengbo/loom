use crate::runtime::{node::NodeRuntime, python::PythonRuntime, Runtime};
use anyhow::Result;

pub fn run(rt: &NodeRuntime, packages: &[String], force: bool, dry_run: bool) -> Result<()> {
    rt.upgrade(packages, force, dry_run)
}

pub fn run_py(rt: &PythonRuntime, packages: &[String], force: bool, dry_run: bool) -> Result<()> {
    rt.upgrade(packages, force, dry_run)
}
