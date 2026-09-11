//! Public workload orchestration and measurements; synthesis and system execution stay in vlrts.
pub mod cases;
pub mod workload;
pub mod measurement;
#[cfg(feature = "engine")] pub mod provider;
#[cfg(feature = "engine")] pub mod build;
#[cfg(feature = "engine")] pub mod vehicle;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("benchmark checkout")
}

pub fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
