//! cocli-plugin-sdk — SDK for plugin authors. M0.1.0 implementation.

#![allow(dead_code)]
pub fn version() -> &'static str { env!("CARGO_PKG_VERSION") }
#[cfg(test)] mod tests { #[test] fn placeholder() { assert_eq!(super::version(), "0.0.0"); } }
