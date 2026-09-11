# Independent inputs and expectations

These are fixed files, not expectations produced by a Vehicle run. Each correctness case names
its input and expected-output file explicitly.

LZ4 decompresses the Canterbury Large Corpus's 1992 CIA World Factbook (`world192.txt`).
The original source and its embedded Project Gutenberg redistribution notice are retained.
The input is one raw LZ4 block produced with lz4_flex 0.14.0. The expected JSON contains five
little-endian result words: compressed length, decompressed length, FNV-1a digest, first word,
last word. Those facts come from the original uncompressed text.

- Compressed SHA-256: `3100d9cb604b2a983a08a618ac308ac547fdd32f2c6a2f8a84d461ada5204741`.
- Original text SHA-256: `1aebdc97d29904b25791da9aa32be90b69d7da6dc0ac9b95512ed27ed40d2112`.
- Source: [Canterbury Large Corpus](https://corpus.canterbury.ac.nz/descriptions/),
  [Project Gutenberg ebook 48](https://www.gutenberg.org/ebooks/48).

WASM parses a fixed compilation of the parser itself: 119833 bytes, 146 bodies, 42834 operators.
Its 88-byte little-endian expectation and adjacent manifest were independently established by
WABT 1.0.39 wasm-objdump. Do not regenerate the module or expectation from the current parser.

- Input SHA-256: `2997be387b112d56f77ef24c63852d3556f14844728f46dcfae7089eb295f92c`.
- Original compiler: rustc 1.98.0-nightly (b354133fb 2026-06-03), LLVM 22.1.6.
- The input contains wasmparser 0.244.0 (`Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT`),
  bitflags 2.13.1 (`MIT OR Apache-2.0`) and Rust toolchain code (`MIT OR Apache-2.0`, with
  applicable LLVM exceptions). WABT is a generator-time tool, not embedded code.

lz4_flex 0.14.0 is MIT-licensed. Dependency license texts remain in the published crate sources.
No license for original benchmark/runtime project code has been selected by this document;
that owner decision is still required before release publication.
