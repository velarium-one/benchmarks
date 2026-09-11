extern crate alloc;

use alloc::{vec, vec::Vec};
use guest_kit::Resources;

mod frame;

/// Evidence over one complete decompression, independent of its publication representation.
pub struct Evidence {
    pub compressed_length: u32,
    pub decompressed_length: u32,
    pub content_fnv1a32: u32,
    pub first_word: u32,
    pub last_word: u32,
}

impl Evidence {
    pub fn comma_separated(&self) -> Vec<u8> {
        alloc::format!("{},{},{},{},{}", self.compressed_length, self.decompressed_length,
            self.content_fnv1a32, self.first_word, self.last_word).into_bytes()
    }
}

pub fn decompress(resources: &mut impl Resources, key: &str, decompressed_length: usize) -> Evidence {
    let compressed = resources.read_resource(key);
    let block = frame::block(&compressed);
    let mut decompressed = vec![0; decompressed_length];
    let actual = lz4_flex::block::decompress_into(block, &mut decompressed)
        .expect("valid LZ4 block");
    assert_eq!(actual, decompressed_length, "decompressed length differs from entry");

    let content_fnv1a32 = decompressed.iter().fold(0x811c_9dc5u32, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
    });

    Evidence {
        // Report the complete input file length, including the frame around its payload.
        compressed_length: u32::try_from(compressed.len()).expect("RV32 resource length"),
        decompressed_length: u32::try_from(actual).expect("RV32 output length"),
        content_fnv1a32,
        first_word: u32::from_le_bytes(decompressed[..4].try_into().expect("four bytes")),
        last_word: u32::from_le_bytes(decompressed[actual - 4..].try_into().expect("four bytes")),
    }
}
