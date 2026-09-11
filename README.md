# Velarium benchmarks

Public guest sources, inputs, independent expectations and a small client for the closed
standalone engine (`libvlrts.so`). Supported host: x86_64 GNU/Linux with GCC and Rust nightly.
The guest target is `riscv32i-unknown-none-elf`; the second native target is
`i686-unknown-linux-musl`. Install those Rust targets before running the demo.

From this workspace:

```sh
cargo build
cargo test -p benchmarks --test correctness
cargo run -p benchmarks --bin demo -- correctness lz4
```

The two `rstest` cases are readable compile → prepare program → create session → prepare
invocation → invoke → compare examples. They use Monolithic lowering, GCC O2, no tracing or
counters, and 8 MiB fresh guest memory. Each case compares the complete result with a separate
committed expectation. See [cases](cases/README.md) for input provenance and notices.

In the private monorepo, ordinary Cargo commands build and stage the matching engine through
the private adapter. Standalone release downloading is deliberately not implemented yet:
an independent checkout reports that acquisition failure, without a private-source fallback.
Compiled clients retain the exact acquired engine path; moving that engine requires rebuilding
or explicitly loading another trusted matching engine.

The client owns dialect services and preloads inputs. Callback guest buffers are trusted,
accessible transport ranges, not arbitrary memory inspection; their raw pointers cannot escape
the callback. Invalid pointers violate the unsafe contract. Traps and unsupported callbacks are
process-fatal. Loading native engines/Vehicles requires trusted matching artifacts; this demo
does not admit arbitrary native libraries safely or negotiate Vehicle versions.

Compilation sends generated C directly to GCC, leaving no named C file or ordinary compiler
source diagnostic. This is artifact privacy, not secrecy from an operator inspecting process
memory or replacing the compiler. Unexpected internal panic diagnostics are not yet covered by
the source-safe error policy. A failed compilation after target deletion leaves no stale Vehicle.

Guest and native workspaces are excluded from ordinary host targets; explicit package-scoped,
locked Cargo builds own their freshness. Native workers do not depend on the client or acquire
the engine. Original project-code licensing and release distribution are still pending owner
decisions; no publication-readiness claim is made here.
