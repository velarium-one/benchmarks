fn main() -> benchmarks::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 || args[0] != "correctness" {
        return Err("usage: demo correctness <lz4|wasm>".into());
    }
    let workload = benchmarks::cases::Workload::parse(&args[1])?;
    let case = benchmarks::cases::Case::standard(workload)?;
    benchmarks::vehicle::correctness(&case)?;
    println!("{}: complete output matches independent expectation", workload.name());
    Ok(())
}
