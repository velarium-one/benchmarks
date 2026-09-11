# WASM Parser Fixture Provenance

This fixture parses a fixed compilation of an earlier version of the parser itself:
119833 bytes, 146 bodies, 42834 operators.
Its 88-byte little-endian expectation and adjacent manifest were independently established by
WABT 1.0.39 wasm-objdump. Preserve this historical input and its independent expectation rather
than regenerating them from the current parser; doing so would change the benchmark workload.

- Input SHA-256: `2997be387b112d56f77ef24c63852d3556f14844728f46dcfae7089eb295f92c`.
- Original compiler: rustc 1.98.0-nightly (b354133fb 2026-06-03), LLVM 22.1.6.
- The input contains wasmparser 0.244.0 (`Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT`),
  bitflags 2.13.1 (`MIT OR Apache-2.0`) and Rust toolchain code (`MIT OR Apache-2.0`, with
  applicable LLVM exceptions). WABT is a generator-time tool, not embedded code.

## Reproduce The Evidence

Run `python3 evidence.py` from this directory with WABT 1.0.39's `wasm-objdump` on `PATH`.
The script uses Python's standard library and WABT; it does not call the guest parser.
It reads the adjacent `input.wasm`, regardless of the working directory, and prints each named
result field followed by `expected_hex`, the 88 expected bytes encoded as hexadecimal.
Compare the named values with `oracle.manifest.txt` and the decoded hex with `expected.bin`.
No files are modified.

Section counts come from `wasm-objdump -h`, imported function counts from `-x`, and operator
counts from `-d`. The script excludes local-variable declarations from the instruction count
and groups instructions using the fixture's categories. The final FNV-1a value hashes the first
21 result fields as little-endian `u32` words; it is not a hash of the WASM input.
