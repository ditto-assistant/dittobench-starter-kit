//! DittoBench miner starter kit.
//!
//! Modules:
//! - [`protocol`]: the validator HTTP wire contract.
//! - [`catalog`]: the Ditto tool catalog.
//! - [`datagen`]: deterministic-per-seed dataset generation.
//! - [`scorer`]: turns harness responses into a score report.
//! - [`baseline`]: the optimizable agent (this is what you tune).

pub mod baseline;
pub mod catalog;
pub mod datagen;
pub mod protocol;
pub mod scorer;
