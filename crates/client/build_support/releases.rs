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

// Archive identities are explicit; acquisition never selects a latest release implicitly.
pub const PINS: &[ReleasePin] = &[
    ReleasePin {
        target: "x86_64-unknown-linux-gnu",
        abi: 2,
        url: "https://github.com/velarium-one/benchmarks/releases/download/0.1.0/libvlrts-0.1.0-abi2-x86_64-unknown-linux-gnu.tar.gz",
        sha256: "2a59473abdf96fec9d17665555b45c17d726cb1e6ff6349bdf0d6a0223c992eb",
    },
    ReleasePin {
        target: "aarch64-unknown-linux-gnu",
        abi: 2,
        url: "https://github.com/velarium-one/benchmarks/releases/download/0.1.0/libvlrts-0.1.0-abi2-aarch64-unknown-linux-gnu.tar.gz",
        sha256: "38cd683c6aff685747c20aeccf379646f9168141c15a4eba6faa0a059b92e0c2",
    },
];

pub fn pin(target: &str, abi: u64) -> Result<&'static ReleasePin, String> {
    PINS.iter().find(|pin| pin.target == target && pin.abi == abi).ok_or_else(|| format!(
        "no published engine pin for ABI {abi}, target {target}"
    ))
}
