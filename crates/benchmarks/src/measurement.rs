use crate::Result;
pub use guest_kit::native::{Sample, NativeRecord};

#[derive(Debug, serde::Serialize)]
pub struct Summary {
    pub total_ns: u64,
    pub sample_count: usize,
    pub mean_ns: f64,
    pub total_instructions: Option<u64>,
}

pub fn summarize(samples: &[Sample], counted: bool) -> Result<Summary> {
    if samples.is_empty() {
        return Err("missing arm samples".into());
    }

    // Admit each duration and its counter availability before publishing the arm summary.
    let mut total_ns = 0u64;
    let mut instructions = 0u64;
    for sample in samples {
        if sample.elapsed_ns == 0 {
            return Err("zero invocation duration".into());
        }
        total_ns = total_ns.checked_add(sample.elapsed_ns)
            .ok_or("duration total overflow")?;

        match (counted, sample.instructions) {
            (false, None) => (),
            (true, Some(value)) if value != 0 => {
                instructions = instructions.checked_add(value)
                    .ok_or("instruction total overflow")?;
            }
            _ => return Err("counter availability differs from arm".into()),
        }
    }

    // Absence of counting remains distinct from a measured instruction total.
    let sample_count = samples.len();
    let mean_ns = total_ns as f64 / sample_count as f64;
    let total_instructions = counted.then_some(instructions);

    Ok(Summary {
        total_ns,
        sample_count,
        mean_ns,
        total_instructions,
    })
}

/// [nb:core] Four accepted arms and their rate calculations. Raw samples remain separate;
/// the estimated uncounted rate uses the matching counted arm's instruction total.
#[derive(Debug, serde::Serialize)]
pub struct Comparison {
    pub host: Summary,
    pub i686: Summary,
    pub uncounted: Summary,
    pub counted: Summary,
    pub vehicle_percent_host: f64,
    pub vehicle_percent_i686: f64,
    pub counted_hz: f64,
    /// Derived from counted instructions and uncounted time, assuming identical guest work.
    pub uncounted_hz_estimate: f64,
    pub counting_overhead_percent: f64,
}

/// Arms must execute the same guest workload/input; only the counted Vehicle measures instructions.
pub fn compare(host: &[Sample], i686: &[Sample], uncounted: &[Sample], counted: &[Sample]) -> Result<Comparison> {
    if [i686.len(), uncounted.len(), counted.len()].iter().any(|n| *n != host.len()) {
        return Err("arms have different sample counts".into());
    }

    let host = summarize(host, false)?;
    let i686 = summarize(i686, false)?;
    let uncounted = summarize(uncounted, false)?;
    let counted = summarize(counted, true)?;

    let vehicle_percent_host = 100.0 * host.mean_ns / uncounted.mean_ns;
    let vehicle_percent_i686 = 100.0 * i686.mean_ns / uncounted.mean_ns;

    let instructions = counted.total_instructions.expect("counted summary") as f64;
    let counted_hz = instructions * 1e9 / counted.total_ns as f64;
    let uncounted_hz_estimate = instructions * 1e9 / uncounted.total_ns as f64;

    let counting_overhead_percent = 100.0 * (counted.mean_ns / uncounted.mean_ns - 1.0);

    Ok(Comparison {
        host,
        i686,
        uncounted,
        counted,
        vehicle_percent_host,
        vehicle_percent_i686,
        counted_hz,
        uncounted_hz_estimate,
        counting_overhead_percent,
    })
}
