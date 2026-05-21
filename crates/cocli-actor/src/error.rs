use thiserror::Error;

#[derive(Debug, Error)]
pub enum ActorError {
    #[error("channel closed")]
    ChannelClosed,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("ws: {0}")]
    Ws(String),
    #[error("driver: {0}")]
    Driver(String),
    #[error("shutdown requested")]
    Shutdown,
}

pub type ActorResult<T = ()> = Result<T, ActorError>;
