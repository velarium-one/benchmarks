use std::{collections::BTreeMap, env, fs, path::{Path, PathBuf}};

// The catalog names fixtures; Cargo binary ownership is checked by the test's Entry lookup.
fn fixture_cases(guest: &Path) -> BTreeMap<String, PathBuf> {
    let pattern = guest.join("*/src/bin/*/fixture.toml");
    let mut cases = BTreeMap::new();

    for fixture in glob::glob(pattern.to_str().expect("UTF-8 guest path")).expect("fixture glob") {
        let fixture = fixture.expect("read fixture path");
        if !fixture.is_file() { continue; }

        let relative = fixture.strip_prefix(guest).expect("fixture inside guest directory");
        let parts: Vec<_> = relative.iter().map(|part| part.to_str().expect("UTF-8 fixture path")).collect();
        let [family, "src", "bin", entry, "fixture.toml"] = parts.as_slice() else {
            panic!("unexpected fixture path: {}", relative.display());
        };
        if family.starts_with('.') || entry.starts_with('.') { continue; }

        let name = case_name(family, entry);
        if let Some(previous) = cases.insert(name.clone(), fixture.clone()) {
            panic!("duplicate test name {name}: {} and {}", previous.display(), fixture.display());
        }
    }

    assert!(!cases.is_empty(), "no guest fixtures found");
    cases
}

fn case_name(family: &str, entry: &str) -> String {
    let mut name: String = format!("{family}_{entry}").chars()
        .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' }).collect();
    if name.starts_with(|character: char| character.is_ascii_digit()) {
        name.insert(0, '_');
    }
    name
}

fn main() {
    // Rebuild the catalog when fixtures are added, removed or renamed, not just edited.
    println!("cargo::rerun-if-changed=../../guest");
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo package directory"));
    let guest = root.join("../../guest").canonicalize().expect("guest directory");
    let cases = fixture_cases(&guest);

    // Attach named rstest cases while leaving the execution workflow in the public test source.
    let mut source = String::from("macro_rules! fixture_cases { ($test:item) => {\n#[rstest::rstest]\n");
    for (name, fixture) in cases {
        source.push_str(&format!("#[case::{name}({:?})]\n", fixture.to_str().expect("UTF-8 fixture path")));
    }
    source.push_str("$test\n}; }\n");

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    fs::write(output.join("fixture_cases.rs"), source).expect("write fixture catalog");
}
