//! [nb:core] Selects an engine source and obtains one exact build-owned artifact.
//! Private package topology and build settings belong to the selected private adapter.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MARKER: &str = ".velarium-public-build";
const TARGET: &str = "x86_64-unknown-linux-gnu";

#[derive(Debug)]
pub enum Source {
    Private { marker: PathBuf, adapter: PathBuf },
    Release,
}

pub fn source(manifest: &Path) -> Result<Source, String> {
    let public_root = manifest.parent().and_then(Path::parent)
        .ok_or("client package must reside under the public workspace's crates directory")?;
    for root in public_root.ancestors().skip(1) {
        let marker = root.join(MARKER);
        match fs::symlink_metadata(&marker) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("cannot inspect {}: {error}", marker.display())),
            Ok(_) => {}
        }
        let declaration = fs::read_to_string(&marker)
            .map_err(|error| format!("cannot read {}: {error}", marker.display()))?;
        let relative = declaration.trim();
        if relative.is_empty() || relative.contains(['\n', '\r']) || Path::new(relative).is_absolute() {
            return Err(format!("malformed private marker: {}", marker.display()));
        }
        let adapter = root.join(relative).canonicalize()
            .map_err(|error| format!("private adapter {relative}: {error}"))?;
        let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
        if !adapter.starts_with(canonical_root) || !adapter.is_file() {
            return Err("private adapter must be a file within the marked root".into());
        }
        return Ok(Source::Private { marker, adapter });
    }
    Ok(Source::Release)
}

/// [nb:entry] Acquires the selected engine without a release fallback after private failure.
pub fn acquire(manifest: &Path, output: &Path, host: &str, target: &str) -> Result<PathBuf, String> {
    // Reject unsupported execution platforms before calling any source adapter.
    if host != TARGET || target != TARGET {
        return Err(format!("unsupported engine host/target: {host}/{target}; required {TARGET}"));
    }
    let Source::Private { marker, adapter } = source(manifest)? else {
        return Err(format!("release acquisition not implemented (ABI {}, target {target})",
            config::ffi::ABI_REVISION));
    };
    println!("cargo:rerun-if-changed={}", marker.display());
    println!("cargo:rerun-if-changed={}", adapter.display());
    // Explicit watches disable Cargo's package-wide default; retain our own implementation inputs.
    println!("cargo:rerun-if-changed={}", manifest.join("build.rs").display());
    println!("cargo:rerun-if-changed={}", manifest.join("build_support").display());

    // Bound all acquisition output to Cargo's provided directory and invalidate stale selection.
    let output = output.canonicalize().map_err(|error| error.to_string())?;
    let directory = output.join("engine");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let artifact = directory.join(format!("libvlrts-local-abi{}-{target}.so", config::ffi::ABI_REVISION));
    match fs::remove_file(&artifact) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("cannot invalidate {}: {error}", artifact.display())),
    }
    if let Err(failure) = build_private(&adapter, &output, &artifact, target) {
        fs::remove_file(&artifact).or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(error) }
        }).map_err(|error| format!("{failure}; cannot remove rejected artifact: {error}"))?;
        return Err(failure);
    }
    println!("cargo:rerun-if-changed={}", artifact.display());
    Ok(artifact)
}

fn build_private(adapter: &Path, output: &Path, artifact: &Path, target: &str) -> Result<(), String> {
    // Execute the private builder without consuming or replacing its inherited jobserver.
    let result = Command::new(adapter).args(["--protocol", "1", "--target", target])
        .arg("--work-dir").arg(output).arg("--artifact").arg(artifact)
        .stderr(Stdio::inherit()).output()
        .map_err(|error| format!("cannot run private adapter {}: {error}", adapter.display()))?;
    if !result.status.success() {
        return Err(format!("private adapter failed: {}", result.status));
    }

    // Admit only the watch protocol, then publish the artifact to the client compilation.
    let records = String::from_utf8(result.stdout).map_err(|error| error.to_string())?;
    for line in records.lines() {
        let value = line.strip_prefix("cargo:rerun-if-changed=")
            .or_else(|| line.strip_prefix("cargo:rerun-if-env-changed="))
            .ok_or_else(|| format!("unknown private adapter output: {line}"))?;
        if value.is_empty() || value.contains('\r') {
            return Err("empty or malformed private adapter watch".into());
        }
        println!("{line}");
    }
    let metadata = fs::symlink_metadata(artifact)
        .map_err(|error| format!("private adapter did not produce {}: {error}", artifact.display()))?;
    if !metadata.file_type().is_file() {
        return Err("private adapter artifact must be a regular file".into());
    }
    Ok(())
}
