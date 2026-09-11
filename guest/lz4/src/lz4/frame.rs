/// Borrow the single compressed block in the demo's fixed LZ4 CLI frame profile.
/// This is not a general frame decoder. Header/content checksums are present but unchecked;
/// correctness is established by the fixture's independent output expectation.
pub(super) fn block(frame: &[u8]) -> &[u8] {
    // Establish the fixed header: version 1, independent blocks, content checksum,
    // 4 MiB maximum block size, and no optional content-size/dictionary fields.
    const HEADER: [u8; 6] = [0x04, 0x22, 0x4d, 0x18, 0x64, 0x70];
    assert!(frame.starts_with(&HEADER), "unsupported LZ4 frame profile");

    // Byte 6 is the header checksum. Skip verification, but still require its presence.
    let size_bytes = frame.get(7..11).expect("truncated LZ4 block header");
    let encoded_size = u32::from_le_bytes(size_bytes.try_into().expect("four-byte block size"));
    assert_eq!(encoded_size & 0x8000_0000, 0, "uncompressed LZ4 blocks are unsupported");
    assert!(encoded_size > 0 && encoded_size <= 4 * 1024 * 1024, "invalid LZ4 block size");

    // Establish a bounded payload before handing it to the existing block decoder.
    const PAYLOAD_START: usize = 11;
    let payload_end = PAYLOAD_START.checked_add(encoded_size as usize)
        .expect("LZ4 block extent overflow");
    let payload = frame.get(PAYLOAD_START..payload_end).expect("truncated LZ4 block");

    // Exactly one end marker and one unchecked content checksum must follow the block.
    // Extra blocks, concatenated frames and trailing data are deliberately unsupported.
    let footer = frame.get(payload_end..).expect("missing LZ4 frame footer");
    assert_eq!(footer.len(), 8, "expected a single-block LZ4 frame footer");
    assert_eq!(&footer[..4], &[0, 0, 0, 0], "expected LZ4 frame end marker");

    payload
}
