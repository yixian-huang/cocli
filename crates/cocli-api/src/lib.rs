//! cocli-api — REST + WebSocket handlers.
//!
//! Implementation arrives in milestone M0.0.1.

#![allow(dead_code)]

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
