//! [nb:core] Selects an engine source and obtains one exact build-owned artifact.
//! Private package topology and build settings belong to the selected private adapter.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MARKER: &str = ".velarium-public-build";
#[path = "releases.rs"]
pub mod releases;

#[derive(Debug)]
pub enum Source {
    Private { marker: PathBuf, adapter: PathBuf },
    Release,
}

pub fn source(manifest: &Path) -> Result<Source, String> {
    let public_root = manifest.parent()
        .and_then(Path::parent)
        .ok_or("client package must reside under the public workspace's crates directory")?;

    // Only an explicit marker above the public workspace selects private integration.
    for root in public_root.ancestors().skip(1) {
        let marker = root.join(MARKER);
        match fs::symlink_metadata(&marker) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("cannot inspect {}: {error}", marker.display())),
            Ok(_) => {}
        }

        // Interpret the selected marker as one relative adapter path.
        let declaration = fs::read_to_string(&marker)
            .map_err(|error| format!("cannot read {}: {error}", marker.display()))?;
        let relative = declaration.trim();
        if relative.is_empty() || relative.contains(['\n', '\r']) || Path::new(relative).is_absolute() {
            return Err(format!("malformed private marker: {}", marker.display()));
        }

        // Resolve the adapter before admitting its containment within the marked root.
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
    if host != target || !releases::TARGETS.contains(&target) {
        return Err(format!("unsupported engine host/target: {host}/{target}; require native x64 or ARM64 GNU/Linux"));
    }

    // Source selection precedes acquisition. Both sources own one exact staged product.
    println!("cargo:rerun-if-changed={}", manifest.join("build.rs").display());
    println!("cargo:rerun-if-changed={}", manifest.join("build_support").display());
    let selected = source(manifest)?;
    let output = output.canonicalize().map_err(|error| error.to_string())?;
    let directory = output.join("engine");

    let (marker, adapter) = match selected {
        Source::Private { marker, adapter } => (marker, adapter),
        Source::Release => {
            let pin = releases::pin(target, config::ffi::ABI_REVISION)?;
            let artifact = directory.join("libvlrts.so");
            if artifact.exists() {
                fs::remove_file(&artifact).map_err(|error| error.to_string())?;
            }

            let status = Command::new("sh").arg(manifest.join("build_support/acquire.sh"))
                .args([pin.url, pin.sha256]).arg(&directory)
                .stdout(Stdio::inherit()).stderr(Stdio::inherit())
                .status().map_err(|error| format!("cannot run engine installer: {error}"))?;
            if !status.success() {
                return Err(format!("engine installer failed: {status}"));
            }

            println!("cargo:rerun-if-changed={}", artifact.display());
            return Ok(artifact);
        }
    };

    // Track source selection alongside this public acquisition implementation.
    println!("cargo:rerun-if-changed={}", marker.display());
    println!("cargo:rerun-if-changed={}", adapter.display());

    // Bound all acquisition output to Cargo's provided directory and invalidate stale selection.
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;

    let artifact = directory.join(format!("libvlrts-local-abi{}-{target}.so", config::ffi::ABI_REVISION));
    match fs::remove_file(&artifact) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("cannot invalidate {}: {error}", artifact.display())),
    }

    // A rejected build must leave no artifact eligible for a subsequent client launch.
    if let Err(failure) = build(&adapter, &output, &artifact, target) {
        let cleanup = match fs::remove_file(&artifact) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        };
        cleanup.map_err(|error| format!("{failure}; cannot remove rejected artifact: {error}"))?;

        return Err(failure);
    }

    println!("cargo:rerun-if-changed={}", artifact.display());
    Ok(artifact)
}

fn build(adapter: &Path, output: &Path, artifact: &Path, target: &str) -> Result<(), String> {
    // Execute the private builder without consuming or replacing its inherited jobserver.
    let result = Command::new(adapter)
        .args(["--protocol", "1", "--target", target])
        .arg("--work-dir").arg(output)
        .arg("--artifact").arg(artifact)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("cannot run private adapter {}: {error}", adapter.display()))?;
    if !result.status.success() {
        return Err(format!("private adapter failed: {}", result.status));
    }

    // Admit the adapter's dependency watches without accepting arbitrary Cargo directives.
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

    // Successful execution and valid watches do not themselves establish an artifact.
    let metadata = fs::symlink_metadata(artifact)
        .map_err(|error| format!("private adapter did not produce {}: {error}", artifact.display()))?;
    if !metadata.file_type().is_file() {
        return Err("private adapter artifact must be a regular file".into());
    }

    Ok(())
}
