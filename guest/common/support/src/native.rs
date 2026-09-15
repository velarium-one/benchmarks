//! Native entry bootstrap and per-invocation timing. This crate never acquires the engine.

use std::{path::Path, time::Instant};
use crate::{Resources, fixture::{Case, Result}};

pub struct NativeResources<'a>(pub &'a Case);

impl Resources for NativeResources<'_> {
    fn input_size(&self) -> u32 { u32::try_from(self.0.input.len()).expect("admitted input") }

    fn input_read_u32(&self, index: u32) -> u32 {
        usize::try_from(index).ok().and_then(|index| index.checked_mul(4))
            .and_then(|start| self.0.input.get(start..start.checked_add(4)?))
            .map(|word| u32::from_le_bytes(word.try_into().expect("complete word"))).unwrap_or(0)
    }

    fn read_resource(&mut self, key: &str) -> Vec<u8> {
        self.0.resources.get(key).expect("required entry resource is preloaded").clone()
    }
}

/// Raw invocation duration; an instruction count belongs only to that invocation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Sample { pub elapsed_ns: u64, pub instructions: Option<u64> }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NativeRecord {
    pub target: String,
    pub pointer_width: u32,
    pub snapshot_sha256: String,
    pub expected_sha256: Option<String>,
    pub warmups: usize,
    pub samples: Vec<Sample>,
}

pub fn measure(case: &Case, warmups: usize, repeats: usize,
    mut run: impl FnMut(&mut NativeResources<'_>) -> Vec<u8>) -> Result<NativeRecord>
{
    if repeats == 0 { return Err("sample count must be positive".into()); }
    for _ in 0..warmups {
        case.validate(&run(&mut NativeResources(case)))?;
    }

    let mut samples = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let mut resources = NativeResources(case);
        let start = Instant::now();
        let output = std::hint::black_box(run(std::hint::black_box(&mut resources)));
        let elapsed_ns = u64::try_from(start.elapsed().as_nanos())?;

        case.validate(&output)?;
        if elapsed_ns == 0 { return Err("zero invocation duration".into()); }
        samples.push(Sample { elapsed_ns, instructions: None });
    }

    let target = match (std::env::consts::ARCH, cfg!(target_env = "musl")) {
        ("x86_64", false) => "x86_64-unknown-linux-gnu",
        ("aarch64", false) => "aarch64-unknown-linux-gnu",
        ("x86", true) => "i686-unknown-linux-musl",
        _ => return Err("unsupported native worker target".into()),
    };
    Ok(NativeRecord { target: target.into(), pointer_width: usize::BITS,
        snapshot_sha256: case.snapshot_sha256.clone(),
        expected_sha256: case.expected.as_ref().map(|value| crate::fixture::sha256(value.bytes())),
        warmups, samples })
}

pub fn main(run: impl FnMut(&mut NativeResources<'_>) -> Vec<u8>) -> Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    let manifest = arguments.next().ok_or("usage: <entry> <fixture.toml> <warmups> <samples>")?;
    let warmups = arguments.next().ok_or("missing warmups")?.to_str().ok_or("invalid warmups")?.parse()?;
    let repeats = arguments.next().ok_or("missing samples")?.to_str().ok_or("invalid samples")?.parse()?;
    if arguments.next().is_some() { return Err("unexpected worker argument".into()); }

    let case = Case::load(Path::new(&manifest))?;
    let record = measure(&case, warmups, repeats, run)?;
    eprintln!("{}: {}", case.manifest.display(), case.validation_label());
    println!("{}", serde_json::to_string(&record)?);
    Ok(())
}
