//! cocli-store — SQLite persistence layer.
//!
//! Implementation arrives in milestone M0.0.1. This crate exists as a
//! placeholder so the workspace builds during M0 bootstrap.

#![allow(dead_code)]

/// Stub fn so the crate has at least one symbol, keeps rustc happy.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {
        assert_eq!(super::version(), "0.0.0");
    }
}
