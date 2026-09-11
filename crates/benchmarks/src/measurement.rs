use crate::{Result, cases::Case, workload};
use std::time::Instant;

#[derive(Debug, serde::Serialize)]
pub struct Summary {
    pub total_ns: u64,
    pub sample_count: usize,
    pub mean_ns: f64,
    pub total_instructions: Option<u64>,
}

pub fn summarize(samples: &[Sample], counted: bool) -> Result<Summary> {
    if samples.is_empty() { return Err("missing arm samples".into()); }
    let mut total_ns = 0u64;
    let mut instructions = 0u64;
    for sample in samples {
        if sample.elapsed_ns == 0 { return Err("zero invocation duration".into()); }
        total_ns = total_ns.checked_add(sample.elapsed_ns).ok_or("duration total overflow")?;
        match (counted, sample.instructions) {
            (false, None) => (),
            (true, Some(value)) if value != 0 => {
                instructions = instructions.checked_add(value).ok_or("instruction total overflow")?;
            }
            _ => return Err("counter availability differs from arm".into()),
        }
    }
    Ok(Summary { total_ns, sample_count: samples.len(), mean_ns: total_ns as f64 / samples.len() as f64,
        total_instructions: counted.then_some(instructions) })
}

/// [nb:core] Four accepted arms and their denominator rules. Raw samples remain separate;
/// uncounted time never receives an instruction count borrowed from another invocation.
#[derive(Debug, serde::Serialize)]
pub struct Comparison {
    pub host: Summary,
    pub i686: Summary,
    pub uncounted: Summary,
    pub counted: Summary,
    pub vehicle_percent_host: f64,
    pub vehicle_percent_i686: f64,
    pub counted_hz: f64,
    pub counting_overhead_percent: f64,
}

pub fn compare(host: &[Sample], i686: &[Sample], uncounted: &[Sample], counted: &[Sample]) -> Result<Comparison> {
    if [i686.len(), uncounted.len(), counted.len()].iter().any(|n| *n != host.len()) {
        return Err("arms have different sample counts".into());
    }
    let host = summarize(host, false)?;
    let i686 = summarize(i686, false)?;
    let uncounted = summarize(uncounted, false)?;
    let counted = summarize(counted, true)?;
    Ok(Comparison {
        vehicle_percent_host: 100.0 * host.mean_ns / uncounted.mean_ns,
        vehicle_percent_i686: 100.0 * i686.mean_ns / uncounted.mean_ns,
        counted_hz: counted.total_instructions.expect("counted summary") as f64 * 1e9 / counted.total_ns as f64,
        counting_overhead_percent: 100.0 * (counted.mean_ns / uncounted.mean_ns - 1.0),
        host, i686, uncounted, counted,
    })
}

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
