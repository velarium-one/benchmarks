use crate::{Result, cases::Workload};
use std::hint::black_box;

/// Native arm's guest-equivalent requested work: copy/allocate, process, project, clean up.
/// Snapshot file I/O and oracle checks belong outside this operation.
pub fn invoke(workload: Workload, snapshot: &[u8]) -> Result<Vec<u8>> {
    let input = black_box(snapshot).to_vec();
    let output = match workload {
        Workload::Lz4 => {
            let mut decoded = vec![0; 2_473_400];
            let length = lz4_flex::block::decompress_into(&input, &mut decoded)?;
            if length != decoded.len() { return Err("unexpected LZ4 output length".into()); }
            let digest = decoded.iter().fold(0x811c_9dc5u32,
                |hash, byte| (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193));
            let word = |bytes: &[u8]| u32::from_le_bytes(bytes.try_into().expect("four-byte boundary"));
            [input.len() as u32, length as u32, digest,
                word(&decoded[..4]), word(&decoded[decoded.len() - 4..])]
                .into_iter().flat_map(u32::to_le_bytes).collect()
        }
        Workload::Wasm => riscv_fixture_wasm_parser::parse_module(&input)
            .map_err(|e| format!("WASM parse failed: {e:?}"))?.to_le_bytes().to_vec(),
    };
    Ok(black_box(output))
}
