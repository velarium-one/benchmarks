#!/usr/bin/env python3
"""Print the expected output using fast doubling, independently of the timed loop."""

from pathlib import Path

MODULUS = 1 << 32


def fibonacci_pair(n):
    """Return F(n), F(n + 1) modulo 2^32 in logarithmic recursion depth."""
    if n == 0:
        return 0, 1

    current, following = fibonacci_pair(n // 2)
    even = current * (2 * following - current) % MODULUS
    odd = (current * current + following * following) % MODULUS

    return (odd, (even + odd) % MODULUS) if n % 2 else (even, odd)


def main():
    fields = Path(__file__).with_name("input.txt").read_text().split()
    if len(fields) != 3:
        raise ValueError("expected two seeds and an iteration count")
    first, second, iterations = map(int, fields)
    if any(value < 0 or value >= MODULUS for value in (first, second, iterations)):
        raise ValueError("input fields must be unsigned u32 integers")

    current, following = fibonacci_pair(iterations)
    result = (first * (following - current) + second * current) % MODULUS

    print(f"Seeds: {first}, {second}; iterations: {iterations}")
    print(f"Result modulo 2^32: {result}")
    print(f'expected = "{result.to_bytes(4, "little").hex()}"')


if __name__ == "__main__":
    main()
