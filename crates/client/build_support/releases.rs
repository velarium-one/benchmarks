pub const TARGETS: &[&str] = &["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"];

/// [nb:core] One published engine archive for a native target and public ABI revision.
/// The installer checks the downloaded archive against this SHA-256 before extracting it.
#[derive(Debug)]
pub struct ReleasePin {
    pub target: &'static str,
    pub abi: u64,
    pub url: &'static str,
    pub sha256: &'static str,
}

// Populate only after the release URL and archive SHA-256 are approved.
// An empty table means unpublished, not an unverified download or a latest-release lookup.
pub const PINS: &[ReleasePin] = &[];

pub fn pin(target: &str, abi: u64) -> Result<&'static ReleasePin, String> {
    PINS.iter().find(|pin| pin.target == target && pin.abi == abi).ok_or_else(|| format!(
        "no published engine pin for ABI {abi}, target {target}; release publication is pending"
    ))
}
