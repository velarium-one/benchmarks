#![feature(alloc_error_handler)]
#![no_main]
#![no_std]

use riscv_fixture_allocator::BoundedBumpAllocator;
use riscv_fixture_kernel as kernel;
use riscv_fixture_lz4::{Lz4FixtureConfig, run};

#[global_allocator]
static ALLOCATOR: BoundedBumpAllocator = BoundedBumpAllocator::new();

const CONFIG: Lz4FixtureConfig = Lz4FixtureConfig {
    asset_path: b"tests/assets/riscv/lz4/world192.txt.lz4",
    decompressed_length: 2_473_400,
};

#[unsafe(no_mangle)]
pub extern "C" fn _start(heap_base: u32, heap_length: u32) -> ! {
    run(&ALLOCATOR, heap_base, heap_length, CONFIG)
}

#[alloc_error_handler]
fn allocation_error(_layout: core::alloc::Layout) -> ! {
    kernel::abort()
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    kernel::abort()
}
