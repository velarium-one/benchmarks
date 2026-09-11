use alloc::{vec, vec::Vec};
use riscv_fixture_kernel as kernel;

pub use riscv_fixture_allocator::BoundedBumpAllocator as Allocator;
pub use kernel::abort;

pub struct GuestResources;

impl crate::Resources for GuestResources {
    fn input_size(&self) -> u32 { kernel::input_size() }

    fn input_read_u32(&self, index: u32) -> u32 { kernel::input_read_u32(index) }

    fn read_resource(&mut self, key: &str) -> Vec<u8> {
        let mut query = kernel::FileQueryResult::default();
        if kernel::file_query(key.as_bytes(), &mut query) != kernel::FileStatus::Success {
            abort();
        }

        let mut bytes = vec![0; query.file_size as usize];
        if kernel::file_read(query.handle, &mut bytes) != kernel::FileStatus::Success {
            abort();
        }
        bytes
    }
}

pub fn publish(output: &[u8]) -> ! {
    // invariant: the RV32 target represents this live allocation's pointer and length in u32.
    // Completion is non-returning, so output remains alive until the Vehicle exits.
    kernel::return_slice(output.as_ptr() as u32, output.len() as u32)
}
