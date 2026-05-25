//! ClaudeProcess: per-spawn DriverProcess for claude.

use cocli_driver::{
    DispatchMode, DriverAction, DriverError, DriverProcess, EncodedStdin, Event, InterruptAction,
    MessageKind, OutboundMessage, Result,
};

pub struct ClaudeProcess {
    session_id: Option<String>,
}

impl ClaudeProcess {
    pub fn new() -> Self {
        Self { session_id: None }
    }

    pub fn set_session_id(&mut self, sid: String) {
        self.session_id = Some(sid);
    }
}

impl Default for ClaudeProcess {
    fn default() -> Self {
        Self::new()
    }
}

impl DriverProcess for ClaudeProcess {
    fn dispatch_mode(&self) -> DispatchMode {
        DispatchMode::Persistent
    }

    fn encode_stdin(&mut self, msg: &OutboundMessage) -> Result<EncodedStdin> {
        // Reuse existing encoder. For system messages, prepend marker
        // (preserves Go-side claude system-message behavior).
        let prefix = match msg.kind {
            MessageKind::User => "",
            MessageKind::System => "[system] ",
        };
        let text = format!("{prefix}{}", msg.text);
        let json = crate::stdin::encode_user_message(&text, self.session_id.as_deref());
        Ok(EncodedStdin::Bytes(format!("{json}\n")))
    }

    fn parse_line(&mut self, line: &str) -> Vec<Event> {
        let evs = crate::events::parse_line_to_events(line);
        // Side-channel: latch session_id when we see SessionStarted.
        for e in &evs {
            if let Event::SessionStarted { session_id } = e {
                if self.session_id.is_none() {
                    self.session_id = Some(session_id.clone());
                }
            }
        }
        evs
    }

    fn take_pending_actions(&mut self) -> Vec<DriverAction> {
        // Claude has no driver-originated writes.
        Vec::new()
    }

    fn steer(&mut self, _input: &str) -> Result<EncodedStdin> {
        Err(DriverError::SteerNotSupported)
    }

    fn interrupt(&mut self) -> Result<InterruptAction> {
        // SIGINT to the claude process: claude treats it as "cancel current turn".
        Ok(InterruptAction::SignalSent(
            nix::sys::signal::Signal::SIGINT,
        ))
    }

    fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
}
