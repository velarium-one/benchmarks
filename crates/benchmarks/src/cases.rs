use std::path::Path;
use crate::Result;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub enum Workload { Lz4, Wasm }
impl Workload {
    pub fn name(self) -> &'static str { match self { Self::Lz4 => "lz4", Self::Wasm => "wasm" } }
    pub fn package(self) -> &'static str {
        match self { Self::Lz4 => "lz4_world_u32", Self::Wasm => "wasm_parse_self_u32" }
    }
    pub fn key(self) -> &'static [u8] {
        match self {
            Self::Lz4 => b"tests/assets/riscv/lz4/world192.txt.lz4",
            Self::Wasm => b"tests/assets/riscv/wasm-parser/self_input.wasm",
        }
    }
    pub fn input_path(self) -> &'static str {
        match self { Self::Lz4 => "cases/lz4/input.lz4", Self::Wasm => "cases/wasm/input.wasm" }
    }
    pub fn expected_path(self) -> &'static str {
        match self { Self::Lz4 => "cases/lz4/expected.json", Self::Wasm => "cases/wasm/expected.bin" }
    }
    pub fn parse(name: &str) -> Result<Self> {
        match name { "lz4" => Ok(Self::Lz4), "wasm" => Ok(Self::Wasm), _ => Err("expected lz4 or wasm".into()) }
    }
}

/// Immutable preloaded input and independent output expectation; neither is a timed operation.
pub struct Case {
    pub workload: Workload,
    pub input: Vec<u8>,
    pub expected: Vec<u8>,
    pub input_sha256: String,
}
impl Case {
    pub fn load(workload: Workload, input: &Path, expected: &Path) -> Result<Self> {
        let input = std::fs::read(input)?;
        let input_sha256 = crate::sha256(&input);
        let digest = match workload {
            Workload::Lz4 => "3100d9cb604b2a983a08a618ac308ac547fdd32f2c6a2f8a84d461ada5204741",
            Workload::Wasm => "2997be387b112d56f77ef24c63852d3556f14844728f46dcfae7089eb295f92c",
        };
        if input_sha256 != digest { return Err("input differs from committed oracle identity".into()); }
        let bytes = std::fs::read(expected)?;
        let expected = match workload {
            Workload::Lz4 => serde_json::from_slice::<[u32; 5]>(&bytes)?
                .into_iter().flat_map(u32::to_le_bytes).collect(),
            Workload::Wasm => {
                if bytes.len() != 88 { return Err("WASM expectation must contain 22 words".into()); }
                bytes
            }
        };
        Ok(Self { workload, input, expected, input_sha256 })
    }
    pub fn standard(workload: Workload) -> Result<Self> {
        Self::load(workload, &crate::root().join(workload.input_path()),
            &crate::root().join(workload.expected_path()))
    }
    pub fn validate(&self, output: &[u8]) -> Result<()> {
        if output != self.expected { return Err(format!("{} output differs from independent oracle", self.workload.name()).into()); }
        Ok(())
    }
}
