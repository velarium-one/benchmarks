//! Cargo owns freshness; every request names one package, target and isolated output directory.

use std::{path::PathBuf, process::{Command, Stdio}};
use serde::Serialize;
use crate::{Result, cases::Entry};

pub const X86_64: &str = "x86_64-unknown-linux-gnu";
pub const AARCH64: &str = "aarch64-unknown-linux-gnu";
pub const I686: &str = "i686-unknown-linux-musl";
pub const GUEST: &str = "riscv32i-unknown-none-elf";

/// The native comparisons available on an execution host. Selected targets are required;
/// a build or execution failure must not silently remove a baseline.
pub struct NativeTargets {
    pub host: &'static str,
    pub i686: Option<&'static str>,
}

impl NativeTargets {
    pub fn for_arch(architecture: &str) -> Result<Self> {
        match architecture {
            "x86_64" => Ok(Self { host: X86_64, i686: Some(I686) }),
            "aarch64" => Ok(Self { host: AARCH64, i686: None }),
            _ => Err(format!("unsupported benchmark host architecture: {architecture}").into()),
        }
    }

    pub fn current() -> Result<Self> {
        Self::for_arch(std::env::consts::ARCH)
    }

    pub fn iter(&self) -> impl Iterator<Item = &'static str> {
        std::iter::once(self.host).chain(self.i686)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Product {
    pub path: PathBuf,
    pub sha256: String,
    pub target: String,
    pub cargo_arguments: Vec<String>,
    pub rustflags: String,
}

pub fn guest(entry: &Entry) -> Result<Product> {
    product(entry, GUEST, "-C link-arg=--emit-relocs -C target-feature=+m", "guest")
}

pub fn native(entry: &Entry, target: &str) -> Result<Product> {
    let expected_elf_class = match target {
        X86_64 | AARCH64 => 2, // ELFCLASS64
        I686 => 1, // ELFCLASS32
        _ => return Err("unsupported native target".into()),
    };

    let product = product(entry, target, "", "native")?;
    let bytes = std::fs::read(&product.path)?;

    let is_elf = bytes.get(..4) == Some(b"\x7fELF");
    let class_matches = bytes.get(4) == Some(&expected_elf_class);
    if !is_elf || !class_matches {
        return Err(format!("native product is not the requested ELF class: {target}").into());
    }

    Ok(product)
}

fn product(entry: &Entry, target: &str, flags: &str, directory: &str) -> Result<Product> {
    // Select one Cargo binary and its isolated target directory.
    let package = &entry.package;
    let workspace = entry.workspace.to_string_lossy();
    let output_directory = crate::root().join("target").join(directory);
    let arguments: Vec<String> = [
        "build", "--locked", "--release", "--jobs", "1",
        "--message-format=json-render-diagnostics",
        "--manifest-path", &workspace,
        "--package", package,
        "--bin", &entry.binary,
        "--target", target,
        "--target-dir", &output_directory.to_string_lossy(),
    ].into_iter().map(str::to_owned).collect();

    // Cargo owns freshness; inherited compiler wrappers must not change the requested product.
    let output = Command::new("cargo")
        .args(&arguments)
        .env("RUSTFLAGS", flags)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .stderr(Stdio::inherit())
        .output()?;
    if !output.status.success() {
        return Err(format!("Cargo failed for {package}/{target}: {}", output.status).into());
    }

    // Admit exactly one executable from the requested binary's artifact messages.
    let mut executable = None;
    for line in output.stdout.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()) {
        let message: serde_json::Value = serde_json::from_slice(line)?;
        if message["reason"] != "compiler-artifact" || message["target"]["name"] != entry.binary {
            continue;
        }

        let is_binary = message["target"]["kind"].as_array()
            .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"));
        if !is_binary { continue; }
        let Some(path) = message["executable"].as_str() else { continue; };

        if executable.is_some() {
            return Err("ambiguous Cargo executable products".into());
        }
        executable = Some(PathBuf::from(path));
    }

    // Bind the artifact bytes to the request that produced them.
    let path = executable.ok_or("Cargo supplied no executable product")?.canonicalize()?;
    let sha256 = crate::sha256(&std::fs::read(&path)?);

    Ok(Product {
        path,
        sha256,
        target: target.into(),
        cargo_arguments: arguments,
        rustflags: flags.into(),
    })
}
