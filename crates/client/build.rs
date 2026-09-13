#[path = "build_support/acquisition.rs"]
mod acquisition;

fn main() {
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let host = std::env::var("HOST").unwrap();
    let target = std::env::var("TARGET").unwrap();

    let artifact = acquisition::acquire(&manifest, &output, &host, &target)
        .unwrap_or_else(|error| panic!("engine acquisition failed: {error}"));

    println!("cargo:rustc-env=VLR_ENGINE_PATH={}", artifact.display());
}
