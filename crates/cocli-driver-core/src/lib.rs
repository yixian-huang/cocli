//! Runtime-neutral driver contract shared by cocli runtime adapters.
//!
//! Ported from `cocli-cloud/daemon-rs` production commit `8d590a13`.
//! This crate intentionally has no dependency on cloud protocol, connection,
//! tenant, or persistence types.

#![forbid(unsafe_code)]

pub mod driver;
pub mod error;
pub mod event;
pub mod headless;
pub mod subtraits;
pub mod types;

pub use driver::Driver;
pub use error::DriverError;
pub use event::{DriverEvent, ErrorSeverity, SignalType};
pub use headless::{encode_stdin_turn_exit, prompt_arg};
pub use subtraits::{
    ExitCodeClassifier, ProcessFactory, ProcessInitializer, SessionFileGC, StdinBinder,
    TurnInterruptor,
};
pub use types::{
    normalize_turn_status, BusyDeliveryMode, DriverAgentConfig, EnvPropagation, ExitCodeClass,
    GcStats, MessageMode, PlatformActionTransport, SkillCompatibility, SpawnConfig, TurnStatus,
};
