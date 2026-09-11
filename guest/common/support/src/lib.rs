#![cfg_attr(target_arch = "riscv32", no_std)]

extern crate alloc;

#[cfg(not(target_arch = "riscv32"))]
pub mod fixture;
#[cfg(not(target_arch = "riscv32"))]
pub mod native;
#[cfg(target_arch = "riscv32")]
pub mod guest;

/// [nb:core] Entry-facing input services, independent of the execution target.
/// Each resource read copies preloaded client bytes into a workload-owned allocation.
pub trait Resources {
    fn input_size(&self) -> u32;
    fn input_read_u32(&self, index: u32) -> u32;
    fn read_resource(&mut self, key: &str) -> alloc::vec::Vec<u8>;
}

/// [nb:recipe] Bind one shared entry function to guest startup or a native measurement worker.
/// The entry owns workload choices; this macro owns only platform startup and publication.
#[macro_export]
macro_rules! entry {
    ($run:path) => {
        #[cfg(not(target_arch = "riscv32"))]
        fn main() -> $crate::fixture::Result<()> {
            $crate::native::main(|resources| $run(resources))
        }

        #[cfg(target_arch = "riscv32")]
        #[global_allocator]
        static ALLOCATOR: $crate::guest::Allocator = $crate::guest::Allocator::new();

        #[cfg(target_arch = "riscv32")]
        #[unsafe(no_mangle)]
        pub extern "C" fn _start(heap_base: u32, heap_length: u32) -> ! {
            if unsafe { ALLOCATOR.initialize(heap_base, heap_length) }.is_err() {
                $crate::guest::abort();
            }

            let output = $run(&mut $crate::guest::GuestResources);
            $crate::guest::publish(&output)
        }

        #[cfg(target_arch = "riscv32")]
        #[alloc_error_handler]
        fn allocation_error(_: core::alloc::Layout) -> ! { $crate::guest::abort() }

        #[cfg(target_arch = "riscv32")]
        #[panic_handler]
        fn panic(_: &core::panic::PanicInfo) -> ! { $crate::guest::abort() }
    };
}
