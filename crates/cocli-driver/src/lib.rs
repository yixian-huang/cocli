//! cocli-driver — Driver trait + shared types for runtime adapters.
//!
//! Implementations live in `cocli-driver-claude`, `cocli-driver-codex`,
//! `cocli-driver-gemini`, `cocli-driver-kimi`, `cocli-driver-chatrs`.

#![forbid(unsafe_code)]

pub mod context;
pub mod error;
pub mod event;
pub mod paths;
pub mod traits;

// Re-exports uncommented as each module is implemented in Tasks 2-6.
pub use context::{
    DispatchMode, EncodedStdin, InterruptAction, MessageKind, OutboundMessage, SpawnContext,
};
pub use error::{DriverError, Result};
pub use event::Event;
pub use paths::{ExitClassification, SkillPaths};
pub use traits::{
    BusyDeliveryMode, Driver, DriverAction, DriverProcess, DriverSpawnResult, EnvPropagation,
    RuntimeCapabilities, SkillCompat,
};
