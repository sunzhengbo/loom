use crate::runtime::{node::NodeRuntime, python::PythonRuntime, Runtime};
use anyhow::Result;

pub fn run(rt: &NodeRuntime, packages: &[String], dry_run: bool) -> Result<()> {
    rt.install(packages, false, dry_run)
}

pub fn run_py(rt: &PythonRuntime, packages: &[String], dev: bool, dry_run: bool) -> Result<()> {
    rt.install(packages, dev, dry_run)
}
