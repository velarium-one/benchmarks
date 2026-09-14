//! [nb:core] Entry-local fixture declarations and immutable preloaded invocation resources.
//! Expectations describe comparison, never guest struct layout or workload identity.

use std::{collections::BTreeMap, path::{Path, PathBuf}};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Bytes,
    String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Declaration {
    #[serde(default)]
    expected_format: Format,
    expected: Option<String>,
    expected_src: Option<PathBuf>,
    input_src: Option<PathBuf>,
    input_words: Option<Vec<u32>>,
    #[serde(default)]
    resources: BTreeMap<String, PathBuf>,
    #[serde(default)]
    resource_sha256: BTreeMap<String, String>,
}

#[derive(Debug)]
pub enum Expectation {
    Bytes(Vec<u8>),
    String(String),
}

impl Expectation {
    pub fn bytes(&self) -> &[u8] {
        match self { Self::Bytes(bytes) => bytes, Self::String(text) => text.as_bytes() }
    }

    pub fn validate(&self, output: &[u8]) -> Result<()> {
        let equal = match self {
            Self::Bytes(expected) => output == expected,
            Self::String(expected) => std::str::from_utf8(output)? == expected,
        };
        if !equal { return Err("output differs from independent expectation".into()); }
        Ok(())
    }
}

/// Every resource and the indexed input are loaded before invocation timing starts.
#[derive(Debug)]
pub struct Case {
    pub manifest: PathBuf,
    pub input: Vec<u8>,
    pub resources: BTreeMap<String, Vec<u8>>,
    pub expected: Option<Expectation>,
    pub snapshot_sha256: String,
}

impl Case {
    pub fn load(manifest: &Path) -> Result<Self> {
        // Validate the declaration before reading any of its referenced data.
        let manifest = manifest.canonicalize()?;
        let declaration: Declaration = toml::from_str(&std::fs::read_to_string(&manifest)?)?;
        if declaration.expected.is_some() && declaration.expected_src.is_some() {
            return Err("expected and expected-src cannot coexist".into());
        }
        if declaration.input_words.is_some() && declaration.input_src.is_some() {
            return Err("input-words and input-src cannot coexist".into());
        }
        let directory = manifest.parent().ok_or("fixture has no directory")?;

        let expected_bytes = match (declaration.expected, declaration.expected_src) {
            (Some(text), None) => Some(match declaration.expected_format {
                Format::Bytes => decode_hex(&text)?,
                Format::String => text.into_bytes(),
            }),
            (None, Some(path)) => Some(std::fs::read(relative(directory, &path)?)?),
            (None, None) => None,
            (Some(_), Some(_)) => unreachable!("mutual exclusion checked above"),
        };
        let expected = expected_bytes.map(|bytes| -> Result<_> {
            Ok(match declaration.expected_format {
                Format::Bytes => Expectation::Bytes(bytes),
                Format::String => Expectation::String(String::from_utf8(bytes)?),
            })
        }).transpose()?;

        // Snapshot all invocation inputs, validating any fixture-owned provenance constraints.
        // Both declarations supply the same indexed byte input; words have explicit wire order.
        let input = match (declaration.input_words, declaration.input_src) {
            (Some(words), None) => words.into_iter().flat_map(u32::to_le_bytes).collect(),
            (None, Some(path)) => std::fs::read(relative(directory, &path)?)?,
            (None, None) => Vec::new(),
            (Some(_), Some(_)) => unreachable!("mutual exclusion checked above"),
        };
        u32::try_from(input.len())?;
        let mut resources = BTreeMap::new();
        for (key, path) in declaration.resources {
            if !valid_key(&key) { return Err(format!("invalid resource key: {key}").into()); }
            let bytes = std::fs::read(relative(directory, &path)?)?;
            u32::try_from(bytes.len())?;
            resources.insert(key, bytes);
        }
        for (key, expected_digest) in declaration.resource_sha256 {
            let bytes = resources.get(&key).ok_or("digest names an undeclared resource")?;
            if sha256(bytes) != expected_digest {
                return Err(format!("resource differs from committed oracle identity: {key}").into());
            }
        }

        // Length-delimited, ordered fields distinguish every input/key/resource combination.
        let mut digest = Sha256::new();
        digest_field(&mut digest, &input);
        for (key, bytes) in &resources {
            digest_field(&mut digest, key.as_bytes());
            digest_field(&mut digest, bytes);
        }
        let snapshot_sha256 = format!("{:x}", digest.finalize());

        Ok(Self { manifest, input, resources, expected, snapshot_sha256 })
    }

    pub fn validate(&self, output: &[u8]) -> Result<()> {
        if let Some(expected) = &self.expected { expected.validate(output)?; }
        Ok(())
    }

    pub fn validation_label(&self) -> &'static str {
        if self.expected.is_some() { "independent expectation" } else { "unchecked output" }
    }
}

pub fn valid_key(key: &str) -> bool {
    !key.is_empty() && !key.starts_with('/') && !key.contains('\0')
        && !key.split('/').any(|part| part == "..")
}

fn relative(directory: &Path, path: &Path) -> Result<PathBuf> {
    if path.is_absolute() { return Err("fixture file paths must be relative".into()); }
    Ok(directory.join(path))
}

fn decode_hex(text: &str) -> Result<Vec<u8>> {
    if text.len() % 2 != 0 { return Err("expected hex must contain complete byte pairs".into()); }
    text.as_bytes().chunks_exact(2).map(|pair| {
        let high = (pair[0] as char).to_digit(16).ok_or("invalid expected hex digit")?;
        let low = (pair[1] as char).to_digit(16).ok_or("invalid expected hex digit")?;
        Ok(((high << 4) | low) as u8)
    }).collect()
}

fn digest_field(digest: &mut Sha256, bytes: &[u8]) {
    digest.update(u64::try_from(bytes.len()).expect("host extent fits u64").to_le_bytes());
    digest.update(bytes);
}

pub fn sha256(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
