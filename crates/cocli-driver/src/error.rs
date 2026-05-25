//! Driver-layer error taxonomy.

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("driver IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("spawn failed: {0}")]
    Spawn(String),

    #[error("steer not supported by this driver")]
    SteerNotSupported,

    #[error("steer attempted while no active turn")]
    SteerNoActiveTurn,

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("handshake timeout")]
    HandshakeTimeout,

    #[error("driver runtime not available in registry: {0}")]
    RuntimeNotAvailable(String),

    #[error("driver runtime not allowed by --allow-runtimes: {0}")]
    RuntimeNotAllowed(String),
}

pub type Result<T> = std::result::Result<T, DriverError>;
