# LZ4 World Fixture Provenance

LZ4 decompresses the Canterbury Large Corpus's 1992 CIA World Factbook (`world192.txt`).
The original source and its embedded Project Gutenberg redistribution notice are retained.
The input is a single-block LZ4 frame generated with the LZ4 1.10.0 CLI. The expected value in
[`fixture.toml`](fixture.toml) contains five independent numeric values: complete compressed file
length, decompressed length, FNV-1a digest, first word, last word. The first value comes from the
compressed file size; the remaining values come from the original uncompressed text.
The fixture spells the values as comma-separated decimal text, matching
the entry's published string without spaces or a trailing newline.

- Compressed SHA-256: `32b2b5b5bb6b7d3ff628557672bf9c90a7bf7933c66684a83a97d4cc4370f9ae`.
- Original text SHA-256: `1aebdc97d29904b25791da9aa32be90b69d7da6dc0ac9b95512ed27ed40d2112`.
- Source: [Canterbury Large Corpus](https://corpus.canterbury.ac.nz/descriptions/),
  [Project Gutenberg ebook 48](https://www.gutenberg.org/ebooks/48).

## Reproduce The Evidence

Run `python3 evidence.py` from this directory. The script uses only Python's standard library
and prints the five comma-separated values for `expected`, followed by a terminal newline.
The first and last words are four-byte little-endian integers.

The script assumes `input.lz4` is `lz4 world192.txt`.

## Accepted Frame Profile

The demo reads one frame with one compressed, independent block, a 4 MiB block-size limit, and
the content-checksum field present. Content-size and dictionary fields, block checksums, linked
blocks, uncompressed blocks and concatenated frames are unsupported and rejected. Header and
content checksum values are deliberately not verified; bounds/layout checks and the independent
output comparison remain in place. This is a controlled-fixture reader, not general LZ4 support.
