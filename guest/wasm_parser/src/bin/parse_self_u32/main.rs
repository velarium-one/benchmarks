#![cfg_attr(target_arch = "riscv32", no_std)]
#![cfg_attr(target_arch = "riscv32", no_main)]
#![cfg_attr(target_arch = "riscv32", feature(alloc_error_handler))]

extern crate alloc;

fn run(resources: &mut impl guest_kit::Resources) -> alloc::vec::Vec<u8> {
    let module = resources.read_resource("input.wasm");
    let result = riscv_fixture_wasm_parser::parse_module(&module).expect("valid core WASM module");
    result.to_le_bytes().to_vec()
}

guest_kit::entry!(run);
