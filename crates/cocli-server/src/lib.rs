//! cocli-server — top-level assembly. M0.0.x implementation.

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
