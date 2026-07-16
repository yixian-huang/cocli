//! Optional capabilities exposed through [`crate::Driver::as_process_factory`]
//! and the other `as_*` accessors.

use std::io::Write;
use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;

use crate::driver::Driver;
use crate::error::DriverError;
use crate::types::{ExitCodeClass, GcStats, SpawnConfig};

pub trait ProcessFactory: Send + Sync {
    fn new_process(&self, cfg: &SpawnConfig) -> Box<dyn Driver>;
}

pub trait ProcessInitializer: Send + Sync {
    fn write_init_sequence(&self, stdin: &mut dyn Write) -> std::io::Result<()>;
}

#[async_trait]
pub trait StdinBinder: Send + Sync {
    fn bind_stdin(&self, stdin: tokio::process::ChildStdin);
    async fn write_stdin(&self, bytes: &[u8]) -> std::io::Result<()>;
}

#[async_trait]
pub trait TurnInterruptor: Send + Sync {
    async fn interrupt_turn(&self) -> Result<(), DriverError>;
}

pub trait ExitCodeClassifier: Send + Sync {
    fn classify_exit_code(&self, code: i32) -> ExitCodeClass;
}

pub trait SessionFileGC: Send + Sync {
    fn gc_session_files(&self, home: &Path, max_age: Duration) -> std::io::Result<GcStats>;
}
