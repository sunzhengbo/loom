use crate::runtime::{node::NodeRuntime, python::PythonRuntime, Runtime};
use anyhow::Result;

pub fn run(rt: &NodeRuntime) -> Result<()> {
    for name in rt.list()? {
        println!("{name}");
    }
    Ok(())
}

pub fn run_py(rt: &PythonRuntime) -> Result<()> {
    for name in rt.list()? {
        println!("{name}");
    }
    Ok(())
}
