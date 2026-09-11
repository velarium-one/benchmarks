#!/usr/bin/env python3
"""Print fixture evidence from the original text and compressed file size."""

from pathlib import Path


def fnv1a32(data: bytes) -> int:
    value = 0x811C9DC5  # FNV-1a 32-bit offset basis.
    for byte in data:
        value = ((value ^ byte) * 0x01000193) & 0xFFFFFFFF
    return value


def main() -> None:
    directory = Path(__file__).resolve().parent
    compressed_length = (directory / "input.lz4").stat().st_size
    original = (directory / "world192.txt").read_bytes()

    if len(original) < 4:
        raise ValueError("original text must contain at least one four-byte word")

    evidence = (
        compressed_length,
        len(original),
        fnv1a32(original),
        int.from_bytes(original[:4], "little"),
        int.from_bytes(original[-4:], "little"),
    )
    print(",".join(str(value) for value in evidence))


if __name__ == "__main__":
    main()
