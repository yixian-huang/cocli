use cocli_driver::{DriverError, Result};

#[test]
fn io_error_converts() {
    fn try_io() -> Result<()> {
        std::fs::read_to_string("/path/that/does/not/exist/anywhere")?;
        Ok(())
    }
    let r = try_io();
    assert!(matches!(r, Err(DriverError::Io(_))));
}

#[test]
fn steer_not_supported_error() {
    let e = DriverError::SteerNotSupported;
    assert!(e.to_string().contains("steer"));
}

#[test]
fn handshake_timeout_error() {
    let e = DriverError::HandshakeTimeout;
    assert!(e.to_string().contains("handshake"));
}

#[test]
fn protocol_error_carries_message() {
    let e = DriverError::Protocol("bad frame".into());
    assert!(e.to_string().contains("bad frame"));
}
