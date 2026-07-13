use crate::runtime::{node::NodeRuntime, python::PythonRuntime, Runtime};
use anyhow::Result;

pub fn run(rt: &NodeRuntime, dry_run: bool) -> Result<()> {
    rt.rebuild(dry_run)
}

pub fn run_py(rt: &PythonRuntime, dry_run: bool) -> Result<()> {
    rt.rebuild(dry_run)
}
