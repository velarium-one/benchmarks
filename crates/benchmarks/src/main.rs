fn main() -> benchmarks::Result<()> {
    benchmarks::runner::cli(std::env::args().skip(1))
}
