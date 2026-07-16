use cocli_driver_core::event::{ErrorSeverity, SignalType};
use cocli_driver_core::types::{normalize_turn_status, TurnStatus};
use cocli_driver_core::DriverEvent;

#[test]
fn normalize_turn_status_maps_completed_synonyms() {
    for value in ["completed", "succeeded", "success", "SUCCESS"] {
        assert_eq!(normalize_turn_status(value), TurnStatus::Completed);
    }
}

#[test]
fn normalize_turn_status_maps_cancelled_synonyms() {
    for value in ["cancelled", "canceled", "interrupted"] {
        assert_eq!(normalize_turn_status(value), TurnStatus::Cancelled);
    }
}

#[test]
fn normalize_turn_status_maps_failed_synonyms() {
    for value in ["failed", "error", "errored"] {
        assert_eq!(normalize_turn_status(value), TurnStatus::Failed);
    }
}

#[test]
fn normalize_turn_status_maps_step_limit_synonyms() {
    for value in [
        "max_steps_reached",
        "max_steps",
        "step_limit",
        "step_limit_reached",
    ] {
        assert_eq!(normalize_turn_status(value), TurnStatus::MaxSteps);
    }
}

#[test]
fn normalize_turn_status_preserves_unknown_value() {
    assert_eq!(
        normalize_turn_status("hibernated"),
        TurnStatus::Unknown("hibernated".to_string())
    );
}

#[test]
fn turn_end_carries_runtime_reported_context_window() {
    let event = DriverEvent::TurnEnd {
        status: TurnStatus::Completed,
        input_tokens: 100,
        output_tokens: 50,
        cost_usd: 0.001,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
        context_window_tokens: 256_000,
    };

    let DriverEvent::TurnEnd {
        context_window_tokens,
        ..
    } = event
    else {
        panic!("expected turn end");
    };

    assert_eq!(context_window_tokens, 256_000);
}

#[test]
fn rate_limit_carries_overage_state() {
    let event = DriverEvent::RateLimit {
        limit_type: "five_hour".to_string(),
        status: "limited".to_string(),
        resets_at: 1_234_567_890,
        overage_status: Some("allowed".to_string()),
        overage_resets: Some(1_234_567_999),
        is_using_overage: true,
    };

    let DriverEvent::RateLimit {
        is_using_overage, ..
    } = event
    else {
        panic!("expected rate limit");
    };

    assert!(is_using_overage);
}

#[test]
fn error_carries_severity_and_http_status() {
    let event = DriverEvent::Error {
        message: "rate limited".to_string(),
        code: Some("rate_limit".to_string()),
        severity: Some(ErrorSeverity::Warning),
        http_status: Some(429),
    };

    let DriverEvent::Error {
        severity,
        http_status,
        ..
    } = event
    else {
        panic!("expected error");
    };

    assert_eq!(
        (severity, http_status),
        (Some(ErrorSeverity::Warning), Some(429))
    );
}

#[test]
fn signal_carries_typed_signal_kind() {
    let event = DriverEvent::Signal {
        signal_type: SignalType::Progress,
        data: serde_json::json!({"pct": 50}),
    };

    let DriverEvent::Signal { signal_type, .. } = event else {
        panic!("expected signal");
    };

    assert_eq!(signal_type, SignalType::Progress);
}
