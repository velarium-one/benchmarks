use benchmarks::{build, cases::{Case, Entry}, provider::Provider, vehicle};
use client::{Engine, Outcome, config::compile::Optimization};

// Ordinary cargo test must not compile both large Vehicles concurrently.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[rstest::rstest]
fn correctness(
    #[files("../../guest/*/src/bin/*/fixture.toml")] manifest: std::path::PathBuf,
) -> benchmarks::Result<()> {
    let _serial = SERIAL.lock().expect("previous demo case failed");

    // Establish the fixture, its guest executable and the engine that will compile it.
    let case = Case::load(&manifest)?;
    let entry = Entry::from_fixture(&manifest)?;

    let elf = build::guest(&entry)?;
    let engine = Engine::acquired()?;

    let directory = benchmarks::root().join("target/vehicles");
    std::fs::create_dir_all(&directory)?;

    let vehicle_path = directory.join(format!("{}-test.so", entry.artifact_name()));
    let compilation = engine.compile_program(&elf.path, &vehicle::config(false, Optimization::O2), &vehicle_path)?;

    eprintln!(
        "{} preparation: frontend={}ns lowering={}ns compiler={}ns",
        entry.name(), compilation.frontend_ns, compilation.lowering_ns, compilation.compiler_ns,
    );

    // Prepare execution storage and bind the client-owned fixture services for one invocation.
    // The matching engine just compiled this trusted demo guest, including its buffer contract.
    let program = unsafe { engine.prepare_program(&vehicle_path)? };
    let session = program.create_session(vehicle::GMEM_CAPACITY)?;

    let mut provider = Provider::new(&case, &case.input);
    let bindings = unsafe { provider.bindings() };
    let prepared = session.prepare_invocation(bindings)?;

    let results = prepared.invoke()?;

    // Check completion and counting policy before comparing the independently expected output.
    results.with_results(|view| -> benchmarks::Result<()> {
        assert_eq!(view.outcome, Outcome::Completed);
        assert!(view.counters.is_empty());

        let output = view.output.expect("completed output");
        case.validate(output)
    })??;

    eprintln!("{}: {}", entry.name(), case.validation_label());
    Ok(())
}
