//! [nb:core] Pinned release acquisition. Archive identity admits transport bytes; engine identity
//! admits the only extracted member and every retained staged copy. Neither comes from the download.

use std::{fs, io::{Cursor, Read, Write}, path::{Path, PathBuf}, time::Duration};
use sha2::{Digest, Sha256};

#[path = "releases.rs"]
mod releases;

pub const TARGETS: &[&str] = &["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"];
const MEMBER: &str = "libvlrts.so";

#[derive(Debug, Clone, Copy)]
pub struct ContentIdentity {
    pub bytes: u64,
    pub sha256: [u8; 32],
}

/// [nb:core] One reviewed engine release for one native target and public ABI revision.
/// Archive and extracted-engine identities are independent; a URL is a location, not evidence.
#[derive(Debug, Clone, Copy)]
pub struct ReleasePin<'a> {
    pub version: &'a str,
    pub target: &'a str,
    pub abi: u64,
    pub url: &'a str,
    pub archive: ContentIdentity,
    pub engine: ContentIdentity,
}

pub fn pin(target: &str, abi: u64) -> Result<&'static ReleasePin<'static>, String> {
    select(releases::PINS, target, abi)
}

pub fn select<'a>(pins: &'a [ReleasePin<'a>], target: &str, abi: u64) -> Result<&'a ReleasePin<'a>, String> {
    let mut matches = pins.iter().filter(|pin| pin.target == target && pin.abi == abi);
    let pin = matches.next().ok_or_else(|| format!(
        "no published engine pin for ABI {abi}, target {target}; release publication is pending"
    ))?;
    if matches.next().is_some() {
        return Err(format!("duplicate engine pins for ABI {abi}, target {target}"));
    }
    validate(pin)?;
    Ok(pin)
}

fn validate(pin: &ReleasePin<'_>) -> Result<(), String> {
    let valid_version = !pin.version.is_empty() && pin.version.len() <= 80
        && pin.version.as_bytes()[0].is_ascii_alphanumeric()
        && pin.version.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte));
    if !valid_version || !TARGETS.contains(&pin.target) || pin.abi != config::ffi::ABI_REVISION {
        return Err("invalid release version, target or ABI revision".into());
    }

    let uri: ureq::http::Uri = pin.url.parse().map_err(|_| "invalid engine release URL")?;
    if uri.scheme_str() != Some("https") || uri.host().is_none()
        || uri.authority().is_some_and(|authority| authority.as_str().contains('@')) {
        return Err("engine release URL must use HTTPS without credentials".into());
    }
    if pin.archive.bytes == 0 || pin.engine.bytes == 0
        || pin.archive.bytes.checked_add(1).is_none() || pin.engine.bytes.checked_add(65537).is_none() {
        return Err("invalid release byte bounds".into());
    }
    Ok(())
}

pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(5)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(Some(Duration::from_secs(120)))
        .proxy(None)
        .build().into()
}

pub fn download(agent: &ureq::Agent, pin: &ReleasePin<'_>) -> Result<Vec<u8>, String> {
    let mut response = agent.get(pin.url).header("Accept-Encoding", "identity")
        .call().map_err(|error| format!("engine download failed: {error}"))?;
    if response.status().as_u16() != 200 {
        return Err(format!("engine download returned HTTP {}", response.status()));
    }

    read_bounded(response.body_mut().as_reader(), pin.archive.bytes, "archive")
}

fn read_bounded(reader: impl Read, limit: u64, kind: &str) -> Result<Vec<u8>, String> {
    let bound = limit.checked_add(1).ok_or("invalid release byte bound")?;
    let mut bytes = Vec::new();
    reader.take(bound).read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read engine {kind}: {error}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!("engine {kind} exceeds its byte bound"));
    }
    Ok(bytes)
}

fn verify(bytes: &[u8], identity: ContentIdentity, kind: &str) -> Result<(), String> {
    if bytes.len() as u64 != identity.bytes {
        return Err(format!("engine {kind} length mismatch"));
    }
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    if digest != identity.sha256 {
        return Err(format!("engine {kind} SHA-256 mismatch"));
    }
    Ok(())
}

pub fn extract(pin: &ReleasePin<'_>, compressed: &[u8]) -> Result<Vec<u8>, String> {
    // Admit compressed bytes before decoding; bound tar overhead as well as library bytes.
    validate(pin)?;
    verify(compressed, pin.archive, "archive")?;
    let mut decoder = flate2::bufread::GzDecoder::new(compressed);
    let tar_bytes = read_bounded(&mut decoder, pin.engine.bytes + 65536, "decoded archive")?;
    if !decoder.into_inner().is_empty() {
        return Err("engine archive has trailing gzip data".into());
    }

    // Read data, never unpack paths. Raw iteration exposes extension/link entries for rejection.
    let mut archive = tar::Archive::new(Cursor::new(&tar_bytes));
    let mut entries = archive.entries().map_err(|error| error.to_string())?.raw(true);
    let mut entry = entries.next().ok_or("engine archive has no member")?
        .map_err(|error| error.to_string())?;
    if !entry.header().entry_type().is_file() || entry.path_bytes().as_ref() != MEMBER.as_bytes()
        || entry.header().as_ustar().is_none() || entry.size() != pin.engine.bytes {
        return Err("engine archive must contain only the regular libvlrts.so of the pinned size".into());
    }
    let engine = read_bounded(&mut entry, pin.engine.bytes, "member")?;
    drop(entry);
    if entries.next().is_some() {
        return Err("engine archive contains an extra member".into());
    }

    // A single USTAR header/data record must end with two zero blocks and only zero padding.
    // tar iteration alone stops at the first end marker and would conceal appended records.
    let data_end = 512 + pin.engine.bytes.div_ceil(512) * 512;
    let end = usize::try_from(data_end).map_err(|_| "engine archive does not fit this host")?;
    let trailer = tar_bytes.get(end..).ok_or("engine archive is truncated")?;
    if trailer.len() < 1024 || trailer.iter().any(|byte| *byte != 0) {
        return Err("engine archive has an invalid end marker or trailing data".into());
    }
    verify(&engine, pin.engine, "member")?;
    Ok(engine)
}

/// Stage a complete pinned engine, or retain an already verified copy without a download.
/// The transport supplies archive bytes; it never chooses their identity or the output path.
pub fn stage(pin: &ReleasePin<'_>, directory: &Path,
    fetch: impl FnOnce(&ReleasePin<'_>) -> Result<Vec<u8>, String>) -> Result<PathBuf, String> {
    validate(pin)?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let artifact = directory.join(format!("libvlrts-{}-abi{}-{}.so", pin.version, pin.abi, pin.target));

    // Reuse checks actual bytes, not a sidecar or the previous acquisition's success.
    match fs::symlink_metadata(&artifact) {
        Ok(metadata) => {
            if metadata.file_type().is_file() && metadata.len() == pin.engine.bytes {
                let bytes = fs::read(&artifact).map_err(|error| error.to_string())?;
                if verify(&bytes, pin.engine, "staged copy").is_ok() {
                    return Ok(artifact);
                }
            }
            fs::remove_file(&artifact).map_err(|error| format!("cannot invalidate staged engine: {error}"))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("cannot inspect staged engine: {error}")),
    }

    let compressed = fetch(pin)?;
    let engine = extract(pin, &compressed)?;

    // A temporary file in the same directory keeps failed writes out of the selected path.
    let mut staged = tempfile::NamedTempFile::new_in(directory).map_err(|error| error.to_string())?;
    staged.write_all(&engine).map_err(|error| error.to_string())?;
    staged.persist(&artifact).map_err(|error| format!("cannot publish staged engine: {error}"))?;
    Ok(artifact)
}
