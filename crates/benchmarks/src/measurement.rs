use crate::{Result, cases::Case, workload};
use std::time::Instant;

/// Raw accepted duration of one invocation; count belongs only to that same invocation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Sample { pub elapsed_ns: u64, pub instructions: Option<u64> }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NativeRecord {
    pub target: String,
    pub pointer_width: u32,
    pub input_sha256: String,
    pub warmups: usize,
    pub samples: Vec<Sample>,
}

pub fn native(case: &Case, warmups: usize, repeats: usize, target: &str) -> Result<NativeRecord> {
    if repeats == 0 { return Err("sample count must be positive".into()); }
    for _ in 0..warmups { case.validate(&workload::invoke(case.workload, &case.input)?)?; }
    let mut samples = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let start = Instant::now();
        let output = workload::invoke(case.workload, &case.input)?;
        let elapsed_ns = u64::try_from(start.elapsed().as_nanos())?;
        case.validate(&output)?;
        if elapsed_ns == 0 { return Err("zero invocation duration".into()); }
        samples.push(Sample { elapsed_ns, instructions: None });
    }
    Ok(NativeRecord { target: target.into(), pointer_width: usize::BITS,
        input_sha256: case.input_sha256.clone(), warmups, samples })
}
