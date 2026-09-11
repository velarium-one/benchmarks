//! Cargo identifies executable entries; adjacent manifests supply their fixtures.

use std::{path::{Path, PathBuf}, process::Command};
use crate::Result;
pub use guest_kit::fixture::{Case, Expectation, Format, valid_key};

/// [nb:core] One Cargo binary and its adjacent fixture, shared by all execution targets.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Entry {
    pub package: String,
    pub binary: String,
    pub family: String,
    pub workspace: PathBuf,
    pub fixture: PathBuf,
}

impl Entry {
    pub fn name(&self) -> String { format!("{}/{}", self.family, self.binary) }

    pub fn artifact_name(&self) -> String {
        // Include package identity without depending on separator characters being unique.
        format!("{}-{}", self.binary, crate::sha256(self.package.as_bytes()))
    }

    pub fn from_fixture(path: &Path) -> Result<Self> {
        let fixture = path.canonicalize()?;
        let package = fixture.ancestors().find(|path| path.join("Cargo.toml").is_file())
            .ok_or("fixture is not inside a Cargo package")?;
        discover(&package.join("Cargo.toml"))?.into_iter()
            .find(|entry| entry.fixture == fixture).ok_or_else(|| "fixture has no adjacent Cargo binary".into())
    }
}

pub fn discover(manifest: &Path) -> Result<Vec<Entry>> {
    // Cargo is the authority for package membership and binary target identity.
    let output = Command::new("cargo").args(["metadata", "--locked", "--no-deps", "--format-version", "1"])
        .arg("--manifest-path").arg(manifest).output()?;
    if !output.status.success() {
        return Err(format!("Cargo entry discovery failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let workspace = PathBuf::from(metadata["workspace_root"].as_str().ok_or("missing workspace root")?)
        .join("Cargo.toml");
    let packages = metadata["packages"].as_array().ok_or("missing Cargo packages")?;

    // Pair executable sources with adjacent fixture declarations, rejecting ambiguous ownership.
    let mut entries = Vec::new();
    for package in packages {
        let name = package["name"].as_str().ok_or("missing package name")?;
        let package_manifest = Path::new(package["manifest_path"].as_str().ok_or("missing package manifest")?);
        let family = package_manifest.parent().and_then(Path::file_name).and_then(|name| name.to_str())
            .ok_or("invalid workload directory")?;

        for target in package["targets"].as_array().ok_or("missing Cargo targets")? {
            if !target["kind"].as_array().ok_or("missing target kind")?.iter().any(|kind| kind == "bin") {
                continue;
            }
            let source = Path::new(target["src_path"].as_str().ok_or("missing binary source")?);
            let fixture = source.parent().ok_or("binary has no directory")?.join("fixture.toml");
            if !fixture.is_file() { continue; }
            let fixture = fixture.canonicalize()?;
            if entries.iter().any(|entry: &Entry| entry.fixture == fixture) {
                return Err("fixture is adjacent to multiple Cargo binary targets".into());
            }

            entries.push(Entry { package: name.into(), binary: target["name"].as_str()
                .ok_or("missing binary name")?.into(), family: family.into(), workspace: workspace.clone(), fixture });
        }
    }
    entries.sort_by_key(Entry::name);
    Ok(entries)
}

pub fn select(selector: &str) -> Result<Vec<Entry>> {
    let entries = discover(&crate::root().join("guest/Cargo.toml"))?;
    let entries: Vec<_> = entries.into_iter().filter(|entry| {
        selector == "all" || selector == entry.name() || selector == entry.family
    }).collect();
    if entries.is_empty() { return Err(format!("no fixture entries match {selector}").into()); }
    Ok(entries)
}
