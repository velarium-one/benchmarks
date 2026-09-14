# Velarium benchmarks

Public guest sources, inputs, independent expectations and a small client for the closed
standalone engine (`libvlrts.so`). Supported host: x86_64 GNU/Linux with GCC and Rust nightly.
The guest target is `riscv32i-unknown-none-elf`; the second native target is
`i686-unknown-linux-musl`. Install those Rust targets before running the demo.

The locally inspected engine requires glibc 2.39 symbols, `libgcc_s.so.1`, and the x86_64 GNU
dynamic loader. It cannot load on Debian 12's older glibc. These are requirements of that binary,
not a tested minimum distribution or kernel for future releases. GCC and its linker remain
external build dependencies; the engine is not a dependency-free shared library.

From this workspace:

```sh
cargo build
cargo test -p benchmarks --test correctness
cargo run --release -p benchmarks --bin bench -- list
cargo run --release -p benchmarks --bin bench -- correctness lz4
cargo run --release -p benchmarks --bin bench -- bench lz4 --warmups 2 --samples 10 --json results.json
cargo run --release -p benchmarks --bin bench -- bench lz4 -O3 --json results-o3.json
```

The discovered `rstest` cases are readable compile → prepare program → create session → prepare
invocation → invoke → compare examples. They use Monolithic lowering, GCC O2, no tracing or
counters, and 8 MiB fresh guest memory. Each case compares the complete result with a separate
committed expectation. See [provenance](guest/PROVENANCE.md) for input identities and notices.

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
memory or replacing the compiler. Before synthesis, the runtime retires the old output into
temporary storage. After retirement, a failed request leaves no stale Vehicle at the selected path.
If retirement itself fails, compilation stops and reports the error.

Vehicle products live under `target/bin/<entry>/vehicle/`, with optimization/counting variants such
as `O2-uncounted.so` and an adjacent `O2-uncounted.sha256`. Each request still runs frontend and C
lowering. The runtime compares the generated C and compiler-request hashes, checks the actual
binary hash, then reuses a match without compiling or linking. Missing or changed products rebuild.
Correctness and the matching benchmark arm share a product. Delete either file to force a rebuild.
The sidecar contains hashes only, not generated C. It guards against accidental edits; it is not
toolchain attestation or protection against an operator deliberately replacing files.

Reported `compiler_ns` is zero on reuse. It measures compile/link work only; compiler identity
queries, cache lookup, hashing and publication are excluded. Frontend and lowering durations still
describe the current request.

The guest source workspace is excluded from ordinary host targets; explicit package/binary/target,
locked Cargo builds own freshness. Its entries also produce native workers, which do not acquire
the engine. Evaluation-only licensing is intended for the original project code and engine;
final terms and release distribution are not yet approved. Third-party components retain their
own licenses. No publication-readiness claim is made here.

## Copy An Entry And Experiment

`guest/common` contains allocator, kernel and cross-target support. `guest/lz4` and
`guest/wasm_parser` contain reusable workload libraries. Their `src/bin/<entry>/` directories each
contain a `main.rs`, `fixture.toml`, input files and independent expectations/provenance.

Copy an entry directory under the same `src/bin`, choose a new directory name, and edit its `run`
function or fixture. Cargo discovers the new binary; the demo discovers its adjacent fixture.
No central registry or separate native implementation needs updating. `bench list` shows selectors;
use `lz4/my_entry` for one entry, `lz4` for that family's entries, or `all` for every fixture.

The same entry is compiled as RV32, x64 GNU and i686 musl. Its workload function receives resources
and returns result bytes. Shared bootstrap supplies `_start` and guest completion on RV32, or
`main` and native measurements on the native targets. Terminal output is not guest completion.
LZ4 returns comma-separated decimal evidence as text; WASM returns its existing 88-byte record.
LZ4 accepts a narrow single-block CLI frame profile without checksum verification; see its
[fixture provenance](guest/lz4/src/bin/world_u32/PROVENANCE.md) for the exact command and limits.

A fixture can contain:

```toml
expected-format = "string"
expected = "1,2,3"

[resources]
"key/requested/by/the/entry" = "input.bin"
```

`bytes` is the default format. Inline `expected = "0dE4"` means bytes `0x0d, 0xe4`; case does not
matter in hexadecimal digits. `string` uses exact UTF-8 equality: no trimming, extra newline,
number interpretation or JSON normalization. Instead of inline `expected`, use
`expected-src = "expected.bin"` for raw bytes, or a UTF-8 text file with string mode. Both fields
together are an error; neither means unchecked output, explicitly labelled in tests and reports.
Build/invocation failures still fail. There is no array or JSON comparison mode.

All file paths are relative to `fixture.toml`. Resource keys are exact dialect keys, not paths
opened during invocation. Optional `input-src` provides the separate indexed invocation input.
All resources/input/expectations are preloaded outside timing. The optional `[resource-sha256]`
map pins stock resource identities: update or remove the corresponding pin deliberately when
tinkering with an input. Keep an independently established expectation if claiming correctness.

Public tests use rstest's `#[files]` discovery over entry-local fixture paths, with path-derived names.
The build script watches the guest tree, so adding/removing fixtures refreshes test cases under
ordinary Cargo commands. Large tests still have the compilation cost described below; use a narrow
test filter or entry selector when experimenting.

## Measurements

The `bench` executable is the single driver for listing, correctness and measurements; there is
no separate Cargo benchmark target. Its `bench` subcommand runs measurements. Omit the entry selector to run all discovered
cases. Each comparison has host-native, i686-native, uncounted Vehicle and counted Vehicle arms.
Every sample must match its supplied expectation before it enters the report. Without an
expectation, the report says `unchecked output` and is not independent correctness evidence. Native timings
come from inside each worker, not its process startup or pipe transfer. Counted/uncounted Vehicle
order alternates; reset, preloading, compilation and output checks are outside invocation timing.

Use `-O0`, `-O2` or `-O3` to select GCC optimization for both Vehicle arms; the default is `-O2`.
Supply at most one optimization flag, before or after the entry selector. Native workers keep
their Rust release settings, and correctness commands/tests remain on `-O2`. Console output and
the JSON `vehicle_optimization` field record the selected level; Vehicle filenames include it too.

For mean invocation times `T`, the percentages are `100 * T_native / T_vehicle`: 100% means
equal throughput. Counted frequency is total counted instructions / counted seconds. Uncounted
frequency is a derived estimate: the same instruction total / uncounted seconds, assuming identical
guest work across matching inputs and equal sample counts. It is labelled `derived` in the console
and `uncounted_hz_estimate` in JSON; raw uncounted samples still have no instruction count. Counting overhead
is `100 * (T_counted / T_uncounted - 1)`, including negative observations. No counter from another
invocation is attached to uncounted time. Timer overhead is reported separately, never subtracted.

Console output includes the full raw report; `--json` writes that same record. It includes samples,
stage durations, artifact/configuration/input hashes, actual acquired engine hash and ABI revision,
target build arguments, CPU/affinity, compiler versions and timing exclusions. Native target-default
SIMD and libc allocators differ from the guest's bump allocator; native copy/allocation, processing,
projection and ordinary input cleanup are timed. Guest reclamation occurs during untimed reset.
Pinning is disabled for all arms. No performance threshold is a correctness assertion.

Full four-arm reports have been checked for both stock fixtures at O2 and O3, including independent
expectation validation, raw samples and binary identities. They establish measurements for those
builds; they do not establish execution of the current public rstest examples or a standalone release.
The current integration checks cover tiny Vehicles, stock native oracles and dependency/profile
edits without cleaning. Both stock public rstest cases also passed on 2026-09-14, cold and with native
reuse: LZ4 took 9.94s/0.67s and WASM took 131.19s/28.44s. Both repeats reported zero GCC compilation
time; frontend and lowering still run. These are integrated-checkout results, not standalone release
acceptance.

WASM preparation in the supplied O2 reports took about 123 seconds uncounted and 139 seconds counted,
including frontend, lowering and GCC. An earlier validation attempt exceeded its 120-second GCC cap;
that stopped attempt is not a failure of the later measured runs. The runner itself imposes no such
timeout. Allow for compilation cost when selecting cases; build success alone is not correctness
or benchmark evidence.
