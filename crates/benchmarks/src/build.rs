//! Cargo owns freshness; every request names one package, target and isolated output directory.

use std::{path::{Path, PathBuf}, process::{Command, Stdio}};
use serde::Serialize;
use crate::{Result, cases::Workload};

pub const HOST: &str = "x86_64-unknown-linux-gnu";
pub const I686: &str = "i686-unknown-linux-musl";
pub const GUEST: &str = "riscv32i-unknown-none-elf";

#[derive(Debug, Clone, Serialize)]
pub struct Product {
    pub path: PathBuf,
    pub sha256: String,
    pub target: String,
    pub cargo_arguments: Vec<String>,
    pub rustflags: String,
}

pub fn guest(workload: Workload) -> Result<Product> {
    product(&crate::root().join("guest/Cargo.toml"), workload.package(), GUEST,
        "-C link-arg=--emit-relocs -C target-feature=+m", "guest")
}

pub fn native(target: &str) -> Result<Product> {
    if target != HOST && target != I686 { return Err("unsupported native target".into()); }
    let product = product(&crate::root().join("native/Cargo.toml"), "native-worker", target, "", "native")?;
    let bytes = std::fs::read(&product.path)?;
    let class = if target == I686 { 1 } else { 2 };
    if bytes.get(..4) != Some(b"\x7fELF") || bytes.get(4) != Some(&class) {
        return Err(format!("native product is not the requested ELF class: {target}").into());
    }
    Ok(product)
}

fn product(manifest: &Path, package: &str, target: &str, flags: &str, directory: &str) -> Result<Product> {
    let arguments: Vec<String> = ["build", "--locked", "--release", "--jobs", "1",
        "--message-format=json-render-diagnostics", "--manifest-path", &manifest.to_string_lossy(),
        "--package", package, "--target", target, "--target-dir",
        &crate::root().join("target").join(directory).to_string_lossy()]
        .into_iter().map(str::to_owned).collect();
    let output = Command::new("cargo").args(&arguments).env("RUSTFLAGS", flags)
        .env_remove("CARGO_ENCODED_RUSTFLAGS").env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER").stderr(Stdio::inherit()).output()?;
    if !output.status.success() { return Err(format!("Cargo failed for {package}/{target}: {}", output.status).into()); }
    let mut executable = None;
    for line in output.stdout.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()) {
        let message: serde_json::Value = serde_json::from_slice(line)?;
        if message["reason"] == "compiler-artifact" {
            if let Some(path) = message["executable"].as_str() {
                if executable.is_some() { return Err("ambiguous Cargo executable products".into()); }
                executable = Some(PathBuf::from(path));
            }
        }
    }
    let path = executable.ok_or("Cargo supplied no executable product")?.canonicalize()?;
    Ok(Product { sha256: crate::sha256(&std::fs::read(&path)?), path, target: target.into(),
        cargo_arguments: arguments, rustflags: flags.into() })
}
