use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub root: Option<PathBuf>,
    #[serde(default)]
    pub proxy_url: String,
}

fn main() {
    let cases = [
        "proxy_url = \"http://127.0.0.1:7897\"\nroot = \"C:\\\\Locked\\\\Toml\\\\Path\"",
        "root = \"C:\\\\Locked\\\\Toml\\\\Path\"\nproxy_url = \"http://127.0.0.1:7897\"",
    ];
    for (i, t) in cases.iter().enumerate() {
        let cfg: Result<Config, _> = toml::from_str(t);
        println!("case {}: {:?}", i, cfg);
    }
}