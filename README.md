# Velarium benchmarks

Velarium is a RISC-V execution engine designed for zkVM workloads. Its goal is native-like
performance while preserving RISC-V execution semantics.

It is a whole-program AOT recompiler. Unlike interpreters and JIT binary translators, Velarium
recovers the program’s control flow and register data flow, then recompiles the recovered program as
optimized native code.

This repository lets you compare that execution with native builds of the same program.

As far as we could determine, Velarium is the fastest RISC-V executor available today. If we missed
a faster implementation, please point us to it.

Velarium is available for evaluation and integration.

## Available now and planned

The benchmarks currently run RISC-V programs and optionally count executed guest instructions.
The count provides an execution-progress measure, similar in purpose to timestamps or clocks in
other zkVM implementations; it does not imply the same accounting rules.

Fixtures:

- **LZ4:** decompresses the 2.47 MB CIA World Factbook text and returns a digest and summary values.
- **WASM parser:** parses a fixed, previously compiled version of itself: about 120 KB of WebAssembly,
  146 function bodies and 42,834 operators.
- **Fibonacci:** computes 100 million iterations of the wrapping `u32` recurrence, starting from
  0 and 1.

Planned additions:

- **REVM fixture:** an EVM interpreter workload.
- **Sparse traces:** records of selected events, such as guest memory reads/writes or ABI calls.
- **Dense traces:** instruction-by-instruction execution records.

## Results

### AMD Ryzen 5 5600X

O2, untraced.

| Workload | % of native x64 throughput | % of native i686 throughput | Counted GIPS | Uncounted GIPS | Counting overhead |
|---|---:|---:|---:|---:|---:|
| LZ4 | 34.64% | 54.41% | 5.528 | 6.055 | +9.52% |
| WASM parser | 41.23% | 57.77% | 3.758 | 3.942 | +4.90% |
| Fibonacci | 97.11% | 100.24% | 22.796 | 22.465 | -1.45% |

> [!note]
> Fibonacci's small negative counting overhead is consistent with natural variation in wall-clock time.

### Intel Xeon Platinum 8488C (AWS EC2)

O2, untraced. Ubuntu 26.04, Rust 1.100.0-nightly (2026-09-14), GCC 15.2.0.

| Workload | % of native x64 throughput | % of native i686 throughput | Counted GIPS | Uncounted GIPS | Counting overhead |
|---|---:|---:|---:|---:|---:|
| LZ4 | 36.27% | 45.39% | 3.868 | 4.274 | +10.49% |
| WASM parser | 53.71% | 68.19% | 3.711 | 4.168 | +12.32% |
| Fibonacci | 100.00% | 100.01% | 15.805 | 16.198 | +2.49% |

### AMD EPYC 9R45 (AWS EC2)

O2, untraced. Ubuntu 26.04, Rust 1.100.0-nightly (2026-09-14), GCC 15.2.0.

| Workload | % of native x64 throughput | % of native i686 throughput | Counted GIPS | Uncounted GIPS | Counting overhead |
|---|---:|---:|---:|---:|---:|
| LZ4 | 37.20% | 44.86% | 5.500 | 6.383 | +16.04% |
| WASM parser | 54.00% | 65.80% | 5.848 | 6.361 | +8.76% |
| Fibonacci | 99.80% | 99.82% | 22.260 | 22.247 | -0.06% |

### Arm Neoverse V2 (AWS EC2)

O2, untraced. Ubuntu 26.04, Rust 1.100.0-nightly (2026-09-14), GCC 15.2.0.

| Workload | % of native ARM64 throughput | Counted GIPS | Uncounted GIPS | Counting overhead |
|---|---:|---:|---:|---:|
| LZ4 | 35.42% | 4.091 | 4.916 | +20.15% |
| WASM parser | 47.27% | 2.829 | 2.981 | +5.36% |
| Fibonacci | 86.55% | 12.111 | 12.076 | -0.29% |

### Reading the numbers

**% of** - compares uncounted guest throughput with native throughput; 100% means equal
speed. Native x64 or ARM64 is the ordinary host baseline. On x64, native i686 reduces the width mismatch with RV32,
giving a more isolated view of the overhead. It does not isolate virtualization cost exactly:
register availability, ABI, libraries and allocators also differ between these builds.

**IPS** - guest RISC-V instructions per second; GIPS - billions of IPS. This is execution
throughput; some teams call it "frequency". The compiler can use more efficient native
instructions, and the CPU can execute multiple operations per cycle. GIPS can
therefore exceed the CPU's clock rate in GHz.

The uncounted run executes without instruction counting. The counted run adds that measurement.
Uncounted IPS is derived from the counted run's instruction count and the uncounted run's elapsed
time. Counting overhead is the change in execution time when counting is enabled.

## Inspect it, change it, measure it

The same workload source builds for RV32 and the native host (x64 or ARM64). On x64, it also builds
for native i686. Stock fixtures have independent
expected outputs, and every measured sample must match. Timings cover execution, not compilation,
loading, input preloading, guest memory reset or output checking. Native allocation and ordinary
input cleanup are timed; guest allocations are reclaimed by the untimed reset.

The compiler/runtime engine is closed source and supplied precompiled. The workload sources,
fixtures and measurement harness are here to inspect.

**Don't take our fixtures or harness on trust**:
change the inputs, check the timing boundaries and bring your own workloads. Publish your results,
including unfavorable ones, with the machine details and JSON report. We'd like to see them too.

## Run

Use x86_64 or ARM64 GNU/Linux with glibc 2.39 or newer, GCC, Rust nightly and the
`riscv32i-unknown-none-elf` Rust target. On x86_64, also install `i686-unknown-linux-musl` and
GCC's multilib support. ARM64 compares against ARM64 native only; i686 fields are `null` in its
JSON reports. Both hosts report counted/uncounted guest IPS and counting overhead.

Ordinary Cargo commands download the pinned, precompiled engine automatically, using `sh`, `curl`,
`sha256sum`, `tar`/`gzip` and `mktemp`.

```sh
cargo run --release --bin bench -- list
cargo run --release --bin bench -- bench fib --json fib-results.json
cargo run --release --bin bench -- bench lz4 --json lz4-results.json
cargo run --release --bin bench -- bench wasm_parser --json wasm-results.json
cargo test -p benchmarks --test correctness
```

O2 is the default; pass `-O0` or `-O3` to choose another guest optimization level. The first run
compiles the workload: allow several minutes for WASM. Matching compiled guest binaries are reused
on subsequent runs, though frontend processing and lowering still run.

## Create a fixture

```sh
./new-fixture.sh my_workload
cargo run --release --bin bench -- bench my_workload
```

The script creates `guest/my_workload` with one entry, `src/bin/main`. Pass an optional second
argument to choose another entry name. The starter adds two inputs; replace it with your workload
in `main.rs`. Its adjacent `fixture.toml` supplies the inputs and expected output and explains
the available fields in comments. No separate native implementation or harness registration is needed.

## Contact

If you're interested in evaluating or integrating Velarium for a zkVM, prover, or other RISC-V
workload, contact us at **hello@velarium.one**.

## License

Original non-engine code and documentation are licensed under [Apache-2.0](LICENSE).
The precompiled engine uses the [Velarium Engine Evaluation License 1.0](LICENSE-ENGINE.md):
evaluation and published benchmarks are permitted; production use and reverse engineering are
restricted. Third-party components and assets retain their own licenses.
