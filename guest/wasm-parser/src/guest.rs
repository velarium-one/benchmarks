extern crate alloc;

use alloc::vec;
use riscv_fixture_allocator::BoundedBumpAllocator;
use riscv_fixture_kernel as kernel;

use crate::{WasmFixtureResult, parse_module};

/// Member-specific input to the shared WebAssembly parser fixture operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WasmFixtureConfig {
    pub asset_path: &'static [u8],
}

static mut RESULT: WasmFixtureResult = WasmFixtureResult {
    input_length: 0,
    section_count: 0,
    custom_section_count: 0,
    type_count: 0,
    imported_function_count: 0,
    defined_function_count: 0,
    table_count: 0,
    memory_count: 0,
    global_count: 0,
    export_count: 0,
    element_count: 0,
    data_segment_count: 0,
    code_body_count: 0,
    operator_count: 0,
    control_operator_count: 0,
    call_operator_count: 0,
    parametric_operator_count: 0,
    variable_operator_count: 0,
    memory_operator_count: 0,
    numeric_operator_count: 0,
    reference_table_operator_count: 0,
    facts_fnv1a32: 0,
};

/// Runs one fixture member through shared asset delivery, parsing, and result publication.
pub fn run(
    allocator: &BoundedBumpAllocator,
    heap_base: u32,
    heap_length: u32,
    config: WasmFixtureConfig,
) -> ! {
    if unsafe { allocator.initialize(heap_base, heap_length) }.is_err() {
        kernel::abort();
    }

    // Establish the immutable module snapshot in one exact guest-owned allocation.
    let mut query = kernel::FileQueryResult::default();
    if kernel::file_query(config.asset_path, &mut query) != kernel::FileStatus::Success {
        kernel::abort();
    }
    let mut module = vec![0; query.file_size as usize];
    if kernel::file_read(query.handle, &mut module) != kernel::FileStatus::Success {
        kernel::abort();
    }

    // Traverse borrowed payload views and publish only the fixed structural result.
    let result = parse_module(&module).unwrap_or_else(|_| kernel::abort());
    unsafe {
        let output = core::ptr::addr_of_mut!(RESULT);
        core::ptr::write_volatile(output, result);
        kernel::return_slice(output as u32, WasmFixtureResult::BYTE_LEN as u32);
    }
}
