use benchmarks::{build, cases::{Case, Workload}, provider::Provider, vehicle};
use client::{Engine, Outcome};
use rstest::rstest;

// Ordinary cargo test must not compile both large Vehicles concurrently.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[rstest]
#[case::lz4_world(Workload::Lz4, "cases/lz4/input.lz4", "cases/lz4/expected.json")]
#[case::wasm_self(Workload::Wasm, "cases/wasm/input.wasm", "cases/wasm/expected.bin")]
fn correctness(#[case] workload: Workload, #[case] input: &str, #[case] expected: &str) -> benchmarks::Result<()> {
    let _serial = SERIAL.lock().expect("previous demo case failed");
    let case = Case::load(workload, &benchmarks::root().join(input), &benchmarks::root().join(expected))?;
    let elf = build::guest(workload)?;
    let engine = Engine::acquired()?;
    let directory = benchmarks::root().join("target/vehicles");
    std::fs::create_dir_all(&directory)?;
    let vehicle_path = directory.join(format!("{}-test.so", workload.name()));

    let compilation = engine.compile_program(&elf.path, &vehicle::config(false), &vehicle_path)?;
    eprintln!("{} preparation: frontend={}ns lowering={}ns compiler={}ns", workload.name(),
        compilation.frontend_ns, compilation.lowering_ns, compilation.compiler_ns);
    // The matching engine just compiled this trusted demo guest, including its buffer contract.
    let program = unsafe { engine.prepare_program(&vehicle_path)? };
    let session = program.create_session(vehicle::GMEM_CAPACITY)?;
    let mut provider = Provider::new(&case, &[]);
    let prepared = session.prepare_invocation(unsafe { provider.bindings() })?;
    let results = prepared.invoke()?;
    results.with_results(|view| {
        assert_eq!(view.outcome, Outcome::Completed);
        assert!(view.counters.is_empty());
        assert_eq!(view.output.expect("completed output"), case.expected);
    })?;
    Ok(())
}
