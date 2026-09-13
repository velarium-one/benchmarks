//! Four-arm orchestration: exact preparation products precede separately timed invocations.

use std::{path::PathBuf, process::Command, time::Instant};
use serde::Serialize;
use client::{Engine, config::{compile::Optimization, ffi::ABI_REVISION}};
use crate::{Result, build::{self, Product}, cases::{self, Case, Entry}, measurement::{self, NativeRecord, Sample, Comparison}, vehicle};

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
    pub entry: Entry,
    pub snapshot_sha256: String,
    pub expected_output_sha256: Option<String>,
    pub validation: &'static str,
    pub environment: Environment,
    pub guest: Product,
    pub native_products: [Product; 2],
    pub vehicle_optimization: Optimization,
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
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        return Err(format!("environment query failed: {program}").into());
    }

    let text = String::from_utf8(output.stdout)?;
    Ok(text.trim().to_owned())
}

fn environment() -> Result<Environment> {
    // Establish the execution host and the process's allowed CPUs.
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo")?;
    let cpu = cpuinfo.lines()
        .find(|line| line.starts_with("model name"))
        .ok_or("CPU model unavailable")?
        .to_owned();

    let status = std::fs::read_to_string("/proc/self/status")?;
    let allowed_cpus = status.lines()
        .find(|line| line.starts_with("Cpus_allowed_list:"))
        .ok_or("CPU affinity unavailable")?
        .to_owned();

    // Record tool identities and the exact acquired engine artifact.
    let engine_path = Engine::acquired_path().canonicalize()?;
    let rustc = command_text("rustc", &["-Vv"])?;
    let gcc = command_text("gcc", &["--version"])?;
    let kernel = command_text("uname", &["-srmo"])?;
    let engine_sha256 = crate::sha256(&std::fs::read(&engine_path)?);

    // The harness identity belongs to this executable, not to the engine's API table.
    let harness_label = format!(
        "benchmarks {} {}-bit debug_assertions={}",
        env!("CARGO_PKG_VERSION"), usize::BITS, cfg!(debug_assertions),
    );
    let harness_sha256 = crate::sha256(&std::fs::read(std::env::current_exe()?)?);

    Ok(Environment {
        rustc,
        gcc,
        kernel,
        cpu,
        allowed_cpus,
        affinity_policy: "no runtime pin; native children inherit the same allowed CPUs",
        engine_path,
        engine_sha256,
        abi_revision: ABI_REVISION,
        harness_label,
        harness_sha256,
    })
}

pub fn validate_native(record: &NativeRecord, case: &Case, target: &str, warmups: usize, samples: usize) -> Result<()> {
    let expected_width = match target {
        build::HOST => 64,
        build::I686 => 32,
        _ => return Err("unknown native arm".into()),
    };
    let target_matches = record.target == target && record.pointer_width == expected_width;

    let expected_output_sha256 = case.expected.as_ref().map(|value| crate::sha256(value.bytes()));
    let fixture_matches = record.snapshot_sha256 == case.snapshot_sha256
        && record.expected_sha256 == expected_output_sha256;

    let sampling_matches = record.warmups == warmups && record.samples.len() == samples;
    if !target_matches || !fixture_matches || !sampling_matches {
        return Err("native worker evidence does not match request".into());
    }

    measurement::summarize(&record.samples, false)?;
    Ok(())
}

fn native(product: &Product, case: &Case, warmups: usize, samples: usize) -> Result<NativeRecord> {
    let output = Command::new(&product.path)
        .arg(&case.manifest)
        .arg(warmups.to_string())
        .arg(samples.to_string())
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "native arm {} failed: {}",
            product.target, String::from_utf8_lossy(&output.stderr),
        ).into());
    }

    let record: NativeRecord = serde_json::from_slice(&output.stdout)?;
    validate_native(&record, case, &product.target, warmups, samples)?;

    Ok(record)
}

/// [nb:entry] Prepare exact products, then measure four output-validated arms for one fixed input.
pub fn run(entry: &Entry, optimization: Optimization, warmups: usize, samples: usize) -> Result<Report> {
    if samples == 0 {
        return Err("sample count must be positive".into());
    }

    // Establish all build products and preload the client-owned snapshot before measurement.
    let case = Case::load(&entry.fixture)?;
    let native_products = [build::native(entry, build::HOST)?, build::native(entry, build::I686)?];
    let guest = build::guest(entry)?;

    let environment = environment()?;
    let engine = Engine::acquired()?;

    let directory = crate::root().join("target/vehicles");
    std::fs::create_dir_all(&directory)?;

    // Prepare both Vehicle variants and retain their separate preparation evidence.
    let mut sessions = Vec::new();
    let mut vehicle_products = Vec::new();

    for counted in [false, true] {
        let counting_label = if counted { "counted" } else { "uncounted" };
        eprintln!("compiling {} {counting_label} -{optimization:?}", entry.name());

        let path = directory.join(format!("{}-{optimization:?}-{counting_label}.so", entry.artifact_name()));
        let record = engine.compile_program(&guest.path, &vehicle::config(counted, optimization), &path)?;

        let start = Instant::now();
        // Trusted matching product; no arbitrary native library admission is claimed.
        let program = unsafe { engine.prepare_program(&path)? };
        let program_preparation_ns = u64::try_from(start.elapsed().as_nanos())?;

        let start = Instant::now();
        sessions.push(program.create_session(vehicle::GMEM_CAPACITY)?);
        let session_creation_ns = u64::try_from(start.elapsed().as_nanos())?;

        let digest_hex = |bytes: &[u8]| bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        vehicle_products.push(VehicleProduct {
            path,
            counted,
            elf_sha256: digest_hex(&record.elf_sha256),
            config_sha256: digest_hex(&record.config_sha256),
            vehicle_sha256: digest_hex(&record.vehicle_sha256),
            frontend_ns: record.frontend_ns,
            lowering_ns: record.lowering_ns,
            compiler_ns: record.compiler_ns,
            program_preparation_ns,
            session_creation_ns,
        });
    }

    // Workers report internal durations; startup, pipe transfer and JSON decoding are not timed.
    let host = native(&native_products[0], &case, warmups, samples)?;
    let i686 = native(&native_products[1], &case, warmups, samples)?;

    // Alternate Vehicle order across both warmup and measured invocations.
    // invariant: session order is uncounted, counted because preparation iterates [false, true].
    let [uncounted_session, counted_session] = sessions.as_slice() else {
        unreachable!("both Vehicle variants were prepared");
    };
    let mut uncounted_samples = Vec::new();
    let mut counted_samples = Vec::new();
    let invocation_count = warmups.checked_add(samples).ok_or("invocation count overflow")?;

    for iteration in 0..invocation_count {
        let order = if iteration % 2 == 0 {
            [(uncounted_session, false, &mut uncounted_samples), (counted_session, true, &mut counted_samples)]
        } else {
            [(counted_session, true, &mut counted_samples), (uncounted_session, false, &mut uncounted_samples)]
        };

        for (session, counted, arm_samples) in order {
            let sample = vehicle::sample(session, &case, counted)?;

            if iteration >= warmups {
                arm_samples.push(sample);
            }
        }
    }

    // Compare complete, validated arms; observe timer overhead without subtracting it.
    let comparison = measurement::compare(
        &host.samples, &i686.samples, &uncounted_samples, &counted_samples,
    )?;

    let timer_overhead_ns = (0..100).map(|_| {
        let start = Instant::now();
        std::hint::black_box(());
        u64::try_from(start.elapsed().as_nanos()).expect("timer observation fits u64")
    }).collect();
    // Publish measurements together with the artifacts and conditions they describe.
    Ok(Report {
        entry: entry.clone(),
        snapshot_sha256: case.snapshot_sha256.clone(),
        expected_output_sha256: case.expected.as_ref().map(|value| crate::sha256(value.bytes())),
        validation: case.validation_label(),
        environment,
        guest,
        native_products,
        vehicle_optimization: optimization,
        vehicle_products,
        host,
        i686,
        uncounted_samples,
        counted_samples,
        comparison,
        warmups,
        gmem_capacity: vehicle::GMEM_CAPACITY,
        timer_overhead_ns,
        timing_contract: concat!(
            "Vehicle invoke only, Monolithic GCC, untraced; ",
            "complete accessible memory reset and bindings prepared before timer; ",
            "snapshots/compilation/loading/session setup/oracle checks excluded. ",
            "Native release target-default CPU/SIMD, x64 glibc or i686 musl allocator: ",
            "copy/allocation/work/projection/ordinary input cleanup included. ",
            "Native result Vec allocation is included, result destruction excluded; ",
            "guest bump allocations are reclaimed by untimed session reset. No timer subtraction. ",
            "Uncounted Hz is estimated from the matching counted arm's instruction total and ",
            "uncounted elapsed time, assuming identical guest work; uncounted samples remain count-free.",
        ),
    })
}

pub fn print(report: &Report) {
    let comparison = &report.comparison;

    println!("{} (-{:?}): Vehicle {:.2}% of host-native, {:.2}% of i686; counted {:.3} MHz; uncounted {:.3} MHz (derived); counting overhead {:+.2}%",
        report.entry.name(),
        report.vehicle_optimization,
        comparison.vehicle_percent_host,
        comparison.vehicle_percent_i686,
        comparison.counted_hz / 1e6,
        comparison.uncounted_hz_estimate / 1e6,
        comparison.counting_overhead_percent,
    );
    println!("validation: {}", report.validation);

    let json = serde_json::to_string_pretty(report).expect("serializable complete report");
    println!("{json}");
}

pub fn cli(arguments: impl IntoIterator<Item = String>) -> Result<()> {
    enum Command {
        List,
        Correctness {
            selector: String,
        },
        Benchmark {
            selector: String,
            optimization: Optimization,
            warmups: usize,
            samples: usize,
            json_path: Option<PathBuf>,
        },
    }

    // Establish a complete command before performing any fixture work.
    let mut arguments = arguments.into_iter();
    let command_name = arguments.next().unwrap_or_else(|| "bench".into());

    let command = match command_name.as_str() {
        "list" => {
            if arguments.next().is_some() {
                return Err("unexpected list arguments".into());
            }

            Command::List
        }
        "correctness" => {
            let selector = arguments
                .next()
                .ok_or("correctness requires a family or family/entry")?;

            if arguments.next().is_some() {
                return Err("unexpected correctness arguments".into());
            }

            Command::Correctness { selector }
        }
        "bench" => {
            let mut selector = None;
            let mut optimization = None;
            let mut warmups = 2;
            let mut samples = 10;
            let mut json_path = None;

            while let Some(argument) = arguments.next() {
                match argument.as_str() {
                    "-O0" | "-O2" | "-O3" => {
                        if optimization.is_some() {
                            return Err("specify only one optimization flag".into());
                        }

                        optimization = Some(match argument.as_str() {
                            "-O0" => Optimization::O0,
                            "-O2" => Optimization::O2,
                            "-O3" => Optimization::O3,
                            _ => unreachable!("optimization flag matched above"),
                        });
                    }
                    "--warmups" => {
                        warmups = arguments.next().ok_or("missing warmups")?.parse()?;
                    }
                    "--samples" => {
                        samples = arguments.next().ok_or("missing samples")?.parse()?;
                    }
                    "--json" => {
                        let path = arguments.next().ok_or("missing JSON path")?;
                        json_path = Some(PathBuf::from(path));
                    }
                    _ if !argument.starts_with('-') && selector.is_none() => {
                        selector = Some(argument);
                    }
                    _ => {
                        return Err(format!("unknown or duplicate argument: {argument}").into());
                    }
                }
            }

            if samples == 0 {
                return Err("sample count must be positive".into());
            }

            Command::Benchmark {
                selector: selector.unwrap_or_else(|| "all".into()),
                optimization: optimization.unwrap_or(Optimization::O2),
                warmups,
                samples,
                json_path,
            }
        }
        _ => {
            return Err(concat!(
                "usage: bench list | correctness <family[/entry]> | ",
                "bench [family[/entry]|all] [-O0|-O2|-O3] [--warmups N] [--samples N] [--json PATH]"
            ).into());
        }
    };

    // Execute the selected operation.
    match command {
        Command::List => {
            let entries = cases::select("all")?;

            for entry in entries {
                println!("{}", entry.name());
            }
        }
        Command::Correctness { selector } => {
            let entries = cases::select(&selector)?;

            for entry in entries {
                let case = Case::load(&entry.fixture)?;
                vehicle::correctness(&entry, &case)?;

                println!("{}: {}", entry.name(), case.validation_label());
            }
        }
        Command::Benchmark {
            selector,
            optimization,
            warmups,
            samples,
            json_path,
        } => {
            let entries = cases::select(&selector)?;
            let mut reports = Vec::new();

            for entry in entries {
                let report = run(&entry, optimization, warmups, samples)?;

                print(&report);
                reports.push(report);
            }

            // Publish the complete report set after every selected entry succeeds.
            if let Some(path) = json_path {
                let json = serde_json::to_vec_pretty(&reports)?;
                std::fs::write(path, json)?;
            }
        }
    }

    Ok(())
}
