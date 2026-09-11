use benchmarks::{cases::{Case, Workload}, measurement};

fn main() -> benchmarks::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 { return Err("usage: native-worker <lz4|wasm> <input> <expected> <warmups> <samples>".into()); }
    let workload = Workload::parse(&args[0])?;
    let case = Case::load(workload, args[1].as_ref(), args[2].as_ref())?;
    let target = match (std::env::consts::ARCH, cfg!(target_env = "musl")) {
        ("x86", true) => "i686-unknown-linux-musl",
        ("x86_64", false) => "x86_64-unknown-linux-gnu",
        _ => return Err("unsupported native worker target".into()),
    };
    let record = measurement::native(&case, args[3].parse()?, args[4].parse()?, target)?;
    println!("{}", serde_json::to_string(&record)?);
    Ok(())
}
