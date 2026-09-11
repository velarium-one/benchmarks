#![no_std]

#[cfg(target_arch = "riscv32")]
mod guest;
#[cfg(target_arch = "riscv32")]
pub use guest::{Lz4FixtureConfig, Lz4FixtureResult, run};
