//! engine-core: physics-based aircraft engine simulation.
//!
//! The crate is platform independent (no I/O, no threads) so that the same
//! code runs natively for validation and as WebAssembly inside the browser.
//!
//! Modules:
//! - [`atmosphere`]: ISA atmosphere and inlet total conditions
//! - [`spec`]: engine specification (ratings, geometry, cycle, limits)
//! - [`cycle`]: thermodynamic cycle, nozzle flow, design-point sizing
//! - [`engine`]: time-domain engine model: start, run, shutdown, FADEC,
//!   spool dynamics, sensors and limit monitoring
//! - [`logger`]: fixed-rate CSV data logger

pub mod atmosphere;
pub mod cycle;
pub mod engine;
pub mod logger;
pub mod spec;

pub use engine::{Controls, Engine, EngineState, Environment, Faults, Mode};
pub use spec::EngineSpec;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
