//! cocli-web — embeds React app assets. Real router lands in M0.0.4
//! along with the `embed-web` feature wiring.

#![allow(dead_code)]
pub fn version() -> &'static str { env!("CARGO_PKG_VERSION") }
#[cfg(test)] mod tests { #[test] fn placeholder() { assert_eq!(super::version(), "0.0.0"); } }
