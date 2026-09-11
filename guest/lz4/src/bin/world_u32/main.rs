#![cfg_attr(target_arch = "riscv32", no_std)]
#![cfg_attr(target_arch = "riscv32", no_main)]
#![cfg_attr(target_arch = "riscv32", feature(alloc_error_handler))]

extern crate alloc;

fn run(resources: &mut impl guest_kit::Resources) -> alloc::vec::Vec<u8> {
    let evidence = riscv_fixture_lz4::lz4::decompress(
        resources, "world192.lz4", 2_473_400,
    );
    evidence.comma_separated()
}

guest_kit::entry!(run);
