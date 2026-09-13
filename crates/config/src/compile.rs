//! [nb:core] Caller-owned synthesis request transported as JSON to the closed engine.
//! The request contains declarations, not callbacks, private IR or native compiler arguments.

use serde::{Deserialize, Serialize};
use crate::{abi::RiscvAbi, counters::{CounterConfig, CounterConfigError, validate_counters}};

/// Native representation of the same completed RISC-V control domain.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiscvLoweringMode {
    /// One native function per VIR function, with function-local dynamic routing.
    Monolithic,
    /// One callable per natural region, including grounded regions; no subdivision.
    #[default]
    SsaRegions,
}

impl RiscvLoweringMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Monolithic => "monolithic",
            Self::SsaRegions => "ssa-regions",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Optimization { O0, O2, O3 }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompileConfig {
    pub dialect: RiscvAbi,
    #[serde(default)]
    pub lowering: RiscvLoweringMode,
    pub optimization: Optimization,
    #[serde(default)]
    pub counters: Vec<CounterConfig>,
}

impl CompileConfig {
    /// Decode the complete request; nested ABI deserialization uses its validating builder.
    pub fn from_json(bytes: &[u8]) -> Result<Self, CompileConfigError> {
        let config: Self = serde_json::from_slice(bytes)?;
        validate_counters(&config.counters)?;
        Ok(config)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CompileConfigError {
    #[error("invalid configuration JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid counter declarations: {0}")]
    Counters(#[from] CounterConfigError),
}
