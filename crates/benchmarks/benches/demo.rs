fn main() -> benchmarks::Result<()> {
    benchmarks::runner::cli(std::iter::once("bench".to_owned()).chain(std::env::args().skip(1)))
}
