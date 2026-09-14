#![cfg_attr(target_arch = "riscv32", no_std)]
#![cfg_attr(target_arch = "riscv32", no_main)]
#![cfg_attr(target_arch = "riscv32", feature(alloc_error_handler))]

extern crate alloc;

fn run(resources: &mut impl guest_kit::Resources) -> alloc::vec::Vec<u8> {
    // fixture.toml defines the inputs. Word indices select its first and second u32 values.
    assert_eq!(resources.input_size(), 8, "expected two u32 input words");
    let a = resources.input_read_u32(0);
    let b = resources.input_read_u32(1);

    let sum = a.wrapping_add(b);

    // Return the output bytes; fixture.toml defines the expected value and comparison format.
    // Here the result is one little-endian u32. For UTF-8 text instead, return
    // alloc::format!("{sum}").into_bytes() and set expected-format = "string" in the fixture
    // with expected = "5" for the example inputs.
    sum.to_le_bytes().to_vec()
}

guest_kit::entry!(run);
