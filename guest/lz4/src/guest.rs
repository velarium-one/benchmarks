extern crate alloc;

use alloc::vec;
use riscv_fixture_allocator::BoundedBumpAllocator;
use riscv_fixture_kernel as kernel;

const FNV1A_OFFSET_BASIS: u32 = 0x811c_9dc5;
const FNV1A_PRIME: u32 = 0x0100_0193;

/// Member-specific inputs to the shared LZ4 fixture operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lz4FixtureConfig {
    pub asset_path: &'static [u8],
    pub decompressed_length: usize,
}

/// Compact semantic evidence returned after one complete LZ4 block decompression.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Lz4FixtureResult {
    pub compressed_length: u32,
    pub decompressed_length: u32,
    pub content_fnv1a32: u32,
    pub first_word: u32,
    pub last_word: u32,
}

static mut RESULT: Lz4FixtureResult = Lz4FixtureResult {
    compressed_length: 0,
    decompressed_length: 0,
    content_fnv1a32: 0,
    first_word: 0,
    last_word: 0,
};

/// Runs one fixture member through the shared query, allocation, decompression, and result path.
pub fn run(
    allocator: &BoundedBumpAllocator,
    heap_base: u32,
    heap_length: u32,
    config: Lz4FixtureConfig,
) -> ! {
    if unsafe { allocator.initialize(heap_base, heap_length) }.is_err() {
        kernel::abort();
    }

    // Establish the immutable compressed snapshot in guest-owned memory.
    let mut query = kernel::FileQueryResult::default();
    if kernel::file_query(config.asset_path, &mut query) != kernel::FileStatus::Success {
        kernel::abort();
    }
    let compressed_length = query.file_size as usize;
    let mut compressed = vec![0; compressed_length];
    if kernel::file_read(query.handle, &mut compressed) != kernel::FileStatus::Success {
        kernel::abort();
    }

    // Decode the complete raw block into the member's independently declared output extent.
    let mut decompressed = vec![0; config.decompressed_length];
    let decompressed_length = lz4_flex::block::decompress_into(&compressed, &mut decompressed)
        .unwrap_or_else(|_| kernel::abort());
    if decompressed_length != config.decompressed_length {
        kernel::abort();
    }

    // Publish compact semantic evidence over the decompressed source artifact.
    let result = Lz4FixtureResult {
        compressed_length: query.file_size,
        decompressed_length: decompressed_length as u32,
        content_fnv1a32: fnv1a32(&decompressed),
        first_word: boundary_word(&decompressed[..4]),
        last_word: boundary_word(&decompressed[decompressed.len() - 4..]),
    };
    unsafe {
        let output = core::ptr::addr_of_mut!(RESULT);
        core::ptr::write_volatile(output, result);
        kernel::return_slice(
            output as u32,
            core::mem::size_of::<Lz4FixtureResult>() as u32,
        );
    }
}

fn fnv1a32(bytes: &[u8]) -> u32 {
    bytes.iter().copied().fold(FNV1A_OFFSET_BASIS, |digest, byte| {
        (digest ^ byte as u32).wrapping_mul(FNV1A_PRIME)
    })
}

fn boundary_word(bytes: &[u8]) -> u32 {
    let bytes: [u8; 4] = bytes.try_into().unwrap_or_else(|_| kernel::abort());
    u32::from_le_bytes(bytes)
}
