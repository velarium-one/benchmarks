#![cfg_attr(target_arch = "riscv32", no_std)]
#![cfg_attr(target_arch = "riscv32", no_main)]
#![cfg_attr(target_arch = "riscv32", feature(alloc_error_handler))]

extern crate alloc;

fn run(resources: &mut impl guest_kit::Resources) -> alloc::vec::Vec<u8> {
    let input = resources.read_resource("input");
    let text = core::str::from_utf8(&input).expect("input must be UTF-8");
    let mut words = text.split_ascii_whitespace()
        .map(|word| word.parse::<u32>().expect("input fields must be unsigned u32 integers"));

    let mut current = words.next().expect("missing first seed");
    let mut next = words.next().expect("missing second seed");
    let limit = words.next().expect("missing iteration count");
    assert!(words.next().is_none(), "expected two seeds and an iteration count");

    // Arithmetic is modulo 2^32 on every target, including after the sequence overflows.
    let mut iteration = 0;
    while iteration < limit {
        let sum = current.wrapping_add(next);
        current = next;
        next = sum;
        iteration = iteration.wrapping_add(1);
    }

    current.to_le_bytes().to_vec()
}

guest_kit::entry!(run);
