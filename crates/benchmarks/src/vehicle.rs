//! Demo selection and one validated invocation; no fixture semantics live in the engine.

use client::{Engine, Session, Outcome, config::{compile::*, counters::CounterConfig}};
use crate::{Result, cases::Case, provider::Provider, measurement::Sample};

pub const GMEM_CAPACITY: u64 = 8 * 1024 * 1024;

pub fn config(counted: bool) -> CompileConfig {
    CompileConfig { dialect: client::config::abi::dialects::demo_dialect(),
        lowering: RiscvLoweringMode::Monolithic, optimization: Optimization::O2,
        counters: if counted { vec![CounterConfig::instructions("instructions")] } else { vec![] } }
}

pub fn sample(session: &Session, case: &Case, counted: bool) -> Result<Sample> {
    let mut provider = Provider::new(case, &[]);
    // The copied guest obeys the demo dialect's trusted transport-buffer contract.
    let prepared = session.prepare_invocation(unsafe { provider.bindings() })?;
    let start = std::time::Instant::now();
    let results = prepared.invoke()?;
    let elapsed_ns = u64::try_from(start.elapsed().as_nanos())?;
    if elapsed_ns == 0 { return Err("zero invocation duration".into()); }
    let instructions = results.with_results(|view| -> Result<_> {
        if view.outcome != Outcome::Completed { return Err("Vehicle did not complete".into()); }
        case.validate(view.output.ok_or("completed output missing")?)?;
        match (counted, view.counters) {
            (false, []) => Ok(None),
            (true, [counter]) if counter.name == "instructions" => Ok(Some(counter.value)),
            _ => Err("counter publication differs from requested configuration".into()),
        }
    })??;
    Ok(Sample { elapsed_ns, instructions })
}

pub fn correctness(case: &Case) -> Result<()> {
    let elf = crate::build::guest(case.workload)?;
    let engine = Engine::acquired()?;
    let directory = crate::root().join("target/vehicles");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{}-correctness.so", case.workload.name()));
    engine.compile_program(&elf.path, &config(false), &path)?;
    // Only the trusted artifact just compiled with this engine is admitted.
    let program = unsafe { engine.prepare_program(&path)? };
    let session = program.create_session(GMEM_CAPACITY)?;
    sample(&session, case, false)?;
    Ok(())
}
