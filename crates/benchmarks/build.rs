fn main() {
    // Rstest discovers files during compilation; notify Cargo when the fixture tree changes.
    println!("cargo::rerun-if-changed=../../guest");
}
