//! [nb:core] Ordered declarations of invocation-owned counters compiled into a Vehicle.
//! Names identify results; type, mutation and check specify behavior independently of names.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterType { U64 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterMutation {
    /// Add one per source instruction, batched at source-block close.
    Increment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterCheck { None }

/// Owned transport data. Call `validate_counters` before admitting declarations for compilation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterConfig {
    pub name: String,
    pub value_type: CounterType,
    pub mutation: CounterMutation,
    pub check: CounterCheck,
    #[serde(default)]
    pub initial_value: Option<u64>,
}

impl CounterConfig {
    pub fn instructions(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value_type: CounterType::U64,
            mutation: CounterMutation::Increment,
            check: CounterCheck::None,
            initial_value: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CounterConfigError {
    #[error("counter name must not be empty")]
    EmptyName,
    #[error("duplicate counter name: {0:?}")]
    DuplicateName(String),
}

/// Validate logical identities without imposing backend layout or scratch-capacity rules.
pub fn validate_counters(counters: &[CounterConfig]) -> Result<(), CounterConfigError> {
    let mut names = BTreeSet::new();
    for counter in counters {
        if counter.name.is_empty() { return Err(CounterConfigError::EmptyName); }
        if !names.insert(counter.name.as_str()) {
            return Err(CounterConfigError::DuplicateName(counter.name.clone()));
        }
    }
    Ok(())
}
