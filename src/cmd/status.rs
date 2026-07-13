use crate::runtime::{node::NodeRuntime, python::PythonRuntime, Runtime};
use anyhow::Result;

pub fn run(rt: &NodeRuntime) -> Result<()> {
    let s = rt.status()?;
    if !s.trim().is_empty() {
        print!("{s}");
    }
    Ok(())
}

pub fn run_py(rt: &PythonRuntime) -> Result<()> {
    let s = rt.status()?;
    if !s.trim().is_empty() {
        print!("{s}");
    }
    Ok(())
}
