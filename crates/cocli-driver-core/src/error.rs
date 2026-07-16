//! Errors surfaced by runtime drivers.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DriverError {
    /// Capability not supported by this runtime.
    #[error("not supported by this runtime")]
    Unsupported,

    /// Subprocess I/O error during spawn or stdin write.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// Driver-specific failure with a human-readable message.
    #[error("{0}")]
    Other(String),

    /// Mid-turn steering is fundamentally unsupported by this runtime.
    #[error("turn steer unsupported")]
    TurnSteerUnsupported,

    /// Steering is supported, but the underlying channel is unavailable.
    #[error("turn steer unavailable")]
    TurnSteerUnavailable,

    /// Steering was requested without an active turn.
    #[error("turn steer unavailable: no active turn")]
    TurnSteerNoActiveTurn,
}
