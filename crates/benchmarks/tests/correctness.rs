use benchmarks::{build, cases::{Case, Entry}, provider::Provider, vehicle};
use client::{Engine, Outcome};

include!(concat!(env!("OUT_DIR"), "/fixture_cases.rs"));

// Ordinary cargo test must not compile both large Vehicles concurrently.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fixture_cases! {
    fn correctness(
        #[case] manifest: std::path::PathBuf,
    ) -> benchmarks::Result<()> {
        let _serial = SERIAL.lock().expect("previous demo case failed");
        let case = Case::load(&manifest)?;
        let entry = Entry::from_fixture(&manifest)?;
        let elf = build::guest(&entry)?;
        let engine = Engine::acquired()?;
        let directory = benchmarks::root().join("target/vehicles");
        std::fs::create_dir_all(&directory)?;
        let vehicle_path = directory.join(format!("{}-test.so", entry.artifact_name()));

        let compilation = engine.compile_program(&elf.path, &vehicle::config(false), &vehicle_path)?;
        eprintln!("{} preparation: frontend={}ns lowering={}ns compiler={}ns", entry.name(),
            compilation.frontend_ns, compilation.lowering_ns, compilation.compiler_ns);
        // The matching engine just compiled this trusted demo guest, including its buffer contract.
        let program = unsafe { engine.prepare_program(&vehicle_path)? };
        let session = program.create_session(vehicle::GMEM_CAPACITY)?;
        let mut provider = Provider::new(&case, &case.input);
        let prepared = session.prepare_invocation(unsafe { provider.bindings() })?;
        let results = prepared.invoke()?;
        results.with_results(|view| -> benchmarks::Result<()> {
            assert_eq!(view.outcome, Outcome::Completed);
            assert!(view.counters.is_empty());
            case.validate(view.output.expect("completed output"))
        })??;
        eprintln!("{}: {}", entry.name(), case.validation_label());
        Ok(())
    }
}
