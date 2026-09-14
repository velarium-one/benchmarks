//! Demo selection and one validated invocation; no fixture semantics live in the engine.

use client::{Engine, Session, Outcome, config::{compile::*, counters::CounterConfig}};
use crate::{Result, cases::{Case, Entry}, provider::Provider, measurement::Sample};

pub const GMEM_CAPACITY: u64 = 8 * 1024 * 1024;

/// Shared product location for correctness and benchmark executions of the same variant.
pub fn product_path(entry: &Entry, counted: bool, optimization: Optimization) -> Result<std::path::PathBuf> {
    let directory = crate::root().join("target/bin").join(entry.artifact_name()).join("vehicle");
    std::fs::create_dir_all(&directory)?;

    let counting = if counted { "counted" } else { "uncounted" };
    Ok(directory.join(format!("{optimization:?}-{counting}.so")))
}

pub fn config(counted: bool, optimization: Optimization) -> CompileConfig {
    let dialect = client::config::abi::dialects::demo_dialect();
    let counters = if counted {
        vec![CounterConfig::instructions("instructions")]
    } else {
        vec![]
    };

    CompileConfig {
        dialect,
        lowering: RiscvLoweringMode::Monolithic,
        optimization,
        counters,
    }
}

pub fn sample(session: &Session, case: &Case, counted: bool) -> Result<Sample> {
    let mut provider = Provider::new(case, &case.input);
    // The copied guest obeys the demo dialect's trusted transport-buffer contract.
    let prepared = session.prepare_invocation(unsafe { provider.bindings() })?;

    let start = std::time::Instant::now();
    let results = prepared.invoke()?;
    let elapsed_ns = u64::try_from(start.elapsed().as_nanos())?;
    if elapsed_ns == 0 {
        return Err("zero invocation duration".into());
    }

    let instructions = results.with_results(|view| -> Result<_> {
        if view.outcome != Outcome::Completed {
            return Err("Vehicle did not complete".into());
        }

        let output = view.output.ok_or("completed output missing")?;
        case.validate(output)?;

        match (counted, view.counters) {
            (false, []) => Ok(None),
            (true, [counter]) if counter.name == "instructions" => Ok(Some(counter.value)),
            _ => Err("counter publication differs from requested configuration".into()),
        }
    })??;

    Ok(Sample { elapsed_ns, instructions })
}

pub fn correctness(entry: &Entry, case: &Case, optimization: Optimization) -> Result<()> {
    let elf = crate::build::guest(entry)?;
    let engine = Engine::acquired()?;

    let path = product_path(entry, false, optimization)?;
    let compilation = engine.compile_program(&elf.path, &config(false, optimization), &path)?;

    eprintln!("{} (-{optimization:?}) preparation: frontend={}ns lowering={}ns compiler={}ns",
        entry.name(), compilation.frontend_ns, compilation.lowering_ns, compilation.compiler_ns);

    // Only the trusted artifact compiled or verified for this request is admitted.
    let program = unsafe { engine.prepare_program(&path)? };
    let session = program.create_session(GMEM_CAPACITY)?;

    sample(&session, case, false)?;
    Ok(())
}
