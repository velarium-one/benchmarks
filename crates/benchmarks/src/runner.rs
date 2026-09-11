//! Four-arm orchestration: exact preparation products precede separately timed invocations.

use std::{path::PathBuf, process::Command, time::Instant};
use serde::Serialize;
use client::{Engine, config::ffi::ABI_REVISION};
use crate::{Result, build::{self, Product}, cases::{Case, Workload}, measurement::{self, NativeRecord, Sample, Comparison}, vehicle};

#[derive(Debug, Serialize)]
pub struct VehicleProduct {
    pub path: PathBuf,
    pub counted: bool,
    pub elf_sha256: String,
    pub config_sha256: String,
    pub vehicle_sha256: String,
    pub frontend_ns: u64,
    pub lowering_ns: u64,
    pub compiler_ns: u64,
    pub program_preparation_ns: u64,
    pub session_creation_ns: u64,
}

#[derive(Debug, Serialize)]
pub struct Environment {
    pub rustc: String,
    pub gcc: String,
    pub kernel: String,
    pub cpu: String,
    pub allowed_cpus: String,
    pub affinity_policy: &'static str,
    pub engine_path: PathBuf,
    pub engine_sha256: String,
    pub abi_revision: u64,
    pub harness_label: String,
    pub harness_sha256: String,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub workload: Workload,
    pub input_sha256: String,
    pub expected_output_sha256: String,
    pub environment: Environment,
    pub guest: Product,
    pub native_products: [Product; 2],
    pub vehicle_products: Vec<VehicleProduct>,
    pub host: NativeRecord,
    pub i686: NativeRecord,
    pub uncounted_samples: Vec<Sample>,
    pub counted_samples: Vec<Sample>,
    pub comparison: Comparison,
    pub warmups: usize,
    pub gmem_capacity: u64,
    pub timer_overhead_ns: Vec<u64>,
    pub timing_contract: &'static str,
}

fn command_text(program: &str, args: &[&str]) -> Result<String> {
    let result = Command::new(program).args(args).output()?;
    if !result.status.success() { return Err(format!("environment query failed: {program}").into()); }
    Ok(String::from_utf8(result.stdout)?.trim().to_owned())
}

fn environment() -> Result<Environment> {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo")?;
    let cpu = cpuinfo.lines().find(|line| line.starts_with("model name"))
        .ok_or("CPU model unavailable")?.to_owned();
    let status = std::fs::read_to_string("/proc/self/status")?;
    let allowed_cpus = status.lines().find(|line| line.starts_with("Cpus_allowed_list:"))
        .ok_or("CPU affinity unavailable")?.to_owned();
    let engine_path = Engine::acquired_path().canonicalize()?;
    Ok(Environment { rustc: command_text("rustc", &["-Vv"])?, gcc: command_text("gcc", &["--version"])?,
        kernel: command_text("uname", &["-srmo"])?, cpu, allowed_cpus,
        affinity_policy: "no runtime pin; native children inherit the same allowed CPUs",
        engine_sha256: crate::sha256(&std::fs::read(&engine_path)?), engine_path,
        abi_revision: ABI_REVISION,
        harness_label: format!("benchmarks {} {}-bit debug_assertions={}",
            env!("CARGO_PKG_VERSION"), usize::BITS, cfg!(debug_assertions)),
        harness_sha256: crate::sha256(&std::fs::read(std::env::current_exe()?)?),
    })
}

pub fn validate_native(record: &NativeRecord, case: &Case, target: &str, warmups: usize, samples: usize) -> Result<()> {
    let width = match target { build::HOST => 64, build::I686 => 32, _ => return Err("unknown native arm".into()) };
    if record.target != target || record.pointer_width != width || record.input_sha256 != case.input_sha256
        || record.warmups != warmups || record.samples.len() != samples {
        return Err("native worker evidence does not match request".into());
    }
    measurement::summarize(&record.samples, false)?;
    Ok(())
}

fn native(product: &Product, case: &Case, warmups: usize, samples: usize) -> Result<NativeRecord> {
    let output = Command::new(&product.path).arg(case.workload.name())
        .arg(crate::root().join(case.workload.input_path()))
        .arg(crate::root().join(case.workload.expected_path()))
        .arg(warmups.to_string()).arg(samples.to_string()).output()?;
    if !output.status.success() {
        return Err(format!("native arm {} failed: {}", product.target, String::from_utf8_lossy(&output.stderr)).into());
    }
    let record: NativeRecord = serde_json::from_slice(&output.stdout)?;
    validate_native(&record, case, &product.target, warmups, samples)?;
    Ok(record)
}

/// [nb:entry] Prepare exact products, then measure four output-validated arms for one fixed input.
pub fn run(workload: Workload, warmups: usize, samples: usize) -> Result<Report> {
    if samples == 0 { return Err("sample count must be positive".into()); }
    // Establish all build products and preload the client-owned snapshot before measurement.
    let case = Case::standard(workload)?;
    let native_products = [build::native(build::HOST)?, build::native(build::I686)?];
    let guest = build::guest(workload)?;
    let environment = environment()?;
    let engine = Engine::acquired()?;
    let directory = crate::root().join("target/vehicles");
    std::fs::create_dir_all(&directory)?;
    let mut sessions = Vec::new();
    let mut vehicle_products = Vec::new();
    for counted in [false, true] {
        let path = directory.join(format!("{}-{}.so", workload.name(), if counted { "counted" } else { "uncounted" }));
        eprintln!("compiling {} {}", workload.name(), if counted { "counted" } else { "uncounted" });
        let record = engine.compile_program(&guest.path, &vehicle::config(counted), &path)?;
        let start = Instant::now();
        // Trusted matching product; no arbitrary native library admission is claimed.
        let program = unsafe { engine.prepare_program(&path)? };
        let program_preparation_ns = u64::try_from(start.elapsed().as_nanos())?;
        let start = Instant::now();
        sessions.push(program.create_session(vehicle::GMEM_CAPACITY)?);
        let session_creation_ns = u64::try_from(start.elapsed().as_nanos())?;
        let hex = |bytes: &[u8]| bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        vehicle_products.push(VehicleProduct { path, counted, elf_sha256: hex(&record.elf_sha256),
            config_sha256: hex(&record.config_sha256), vehicle_sha256: hex(&record.vehicle_sha256),
            frontend_ns: record.frontend_ns, lowering_ns: record.lowering_ns, compiler_ns: record.compiler_ns,
            program_preparation_ns, session_creation_ns });
    }

    // Workers report internal durations; startup, pipe transfer and JSON decoding are not timed.
    let host = native(&native_products[0], &case, warmups, samples)?;
    let i686 = native(&native_products[1], &case, warmups, samples)?;
    let mut arms = [Vec::new(), Vec::new()];
    for iteration in 0..warmups.checked_add(samples).ok_or("invocation count overflow")? {
        let order = if iteration % 2 == 0 { [0, 1] } else { [1, 0] };
        for arm in order {
            let sample = vehicle::sample(&sessions[arm], &case, arm == 1)?;
            if iteration >= warmups { arms[arm].push(sample); }
        }
    }

    // Aggregation accepts only complete, validated arms. Timer overhead is observed, not subtracted.
    let [uncounted_samples, counted_samples] = arms;
    let comparison = measurement::compare(&host.samples, &i686.samples, &uncounted_samples, &counted_samples)?;
    let timer_overhead_ns = (0..100).map(|_| {
        let start = Instant::now();
        std::hint::black_box(());
        u64::try_from(start.elapsed().as_nanos()).expect("timer observation fits u64")
    }).collect();
    Ok(Report { workload, input_sha256: case.input_sha256,
        expected_output_sha256: crate::sha256(&case.expected), environment, guest, native_products,
        vehicle_products, host, i686, uncounted_samples, counted_samples, comparison, warmups,
        gmem_capacity: vehicle::GMEM_CAPACITY, timer_overhead_ns,
        timing_contract: "Vehicle invoke only, Monolithic GCC O2, untraced; complete accessible memory reset and bindings prepared before timer; snapshots/compilation/loading/session setup/oracle checks excluded. Native release target-default CPU/SIMD, x64 glibc or i686 musl allocator: copy/allocation/work/projection/ordinary input cleanup included. Native result Vec allocation is included, result destruction excluded; guest bump allocations are reclaimed by untimed session reset. No timer subtraction; counted denominator belongs only to counted invocation." })
}

pub fn print(report: &Report) {
    let comparison = &report.comparison;
    println!("{}: Vehicle {:.2}% of host-native, {:.2}% of i686; counted {:.3} MHz; counting overhead {:+.2}%",
        report.workload.name(), comparison.vehicle_percent_host, comparison.vehicle_percent_i686,
        comparison.counted_hz / 1e6, comparison.counting_overhead_percent);
    println!("{}", serde_json::to_string_pretty(report).expect("serializable complete report"));
}

pub fn cli(arguments: impl IntoIterator<Item = String>) -> Result<()> {
    let mut arguments = arguments.into_iter().filter(|arg| arg != "--bench");
    let command = arguments.next().unwrap_or_else(|| "bench".into());
    if command == "correctness" {
        let workload = Workload::parse(&arguments.next().ok_or("correctness requires lz4 or wasm")?)?;
        if arguments.next().is_some() { return Err("unexpected correctness arguments".into()); }
        vehicle::correctness(&Case::standard(workload)?)?;
        println!("{}: complete output matches independent expectation", workload.name());
        return Ok(());
    }
    if command != "bench" { return Err("usage: demo bench [lz4|wasm|all] [--warmups N] [--samples N] [--json PATH] | correctness <lz4|wasm>".into()); }
    let mut workloads = vec![Workload::Lz4, Workload::Wasm];
    let mut warmups = 2;
    let mut samples = 10;
    let mut json_path = None;
    while let Some(arg) = arguments.next() {
        match arg.as_str() {
            "lz4" | "wasm" => workloads = vec![Workload::parse(&arg)?],
            "all" => workloads = vec![Workload::Lz4, Workload::Wasm],
            "--warmups" => warmups = arguments.next().ok_or("missing warmups")?.parse()?,
            "--samples" => samples = arguments.next().ok_or("missing samples")?.parse()?,
            "--json" => json_path = Some(PathBuf::from(arguments.next().ok_or("missing JSON path")?)),
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if samples == 0 { return Err("sample count must be positive".into()); }
    let mut reports = Vec::new();
    for workload in workloads { let report = run(workload, warmups, samples)?; print(&report); reports.push(report); }
    if let Some(path) = json_path { std::fs::write(path, serde_json::to_vec_pretty(&reports)?)?; }
    Ok(())
}
