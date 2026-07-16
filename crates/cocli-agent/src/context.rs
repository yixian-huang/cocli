//! Context-pressure helpers shared by turn accounting and self-status.

const WARN_PCT: f64 = 0.75;
const RECOVERY_PCT: f64 = 0.70;
const DEFAULT_CRIT_PCT: f64 = 0.90;

pub fn default_backstop_pct(context_window_tokens: i32) -> f64 {
    if context_window_tokens >= 200_000 {
        0.95
    } else {
        DEFAULT_CRIT_PCT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextPressureTier {
    Healthy,
    Warn,
    Crit,
}

impl ContextPressureTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Warn => "warn",
            Self::Crit => "crit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContextPressureStatus {
    pub util_pct: f64,
    pub tier: ContextPressureTier,
    pub fork_suggested: bool,
}

pub fn classify_context_pressure(
    input_tokens: u64,
    context_window_tokens: i32,
    previous_tier: ContextPressureTier,
    backstop_pct: f64,
) -> ContextPressureStatus {
    if context_window_tokens <= 0 || input_tokens == 0 {
        return ContextPressureStatus {
            util_pct: 0.0,
            tier: previous_tier,
            fork_suggested: previous_tier != ContextPressureTier::Healthy,
        };
    }

    let util_pct = (input_tokens as f64 / context_window_tokens as f64).clamp(0.0, 1.0);
    let crit_pct = if backstop_pct > 0.0 {
        backstop_pct
    } else {
        DEFAULT_CRIT_PCT
    };

    let tier = if util_pct >= crit_pct {
        ContextPressureTier::Crit
    } else if util_pct >= WARN_PCT {
        ContextPressureTier::Warn
    } else if previous_tier != ContextPressureTier::Healthy && util_pct >= RECOVERY_PCT {
        previous_tier
    } else {
        ContextPressureTier::Healthy
    };

    ContextPressureStatus {
        util_pct,
        tier,
        fork_suggested: tier != ContextPressureTier::Healthy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_context_pressure_matches_go_thresholds_and_hysteresis() {
        let healthy =
            classify_context_pressure(149_999, 200_000, ContextPressureTier::Healthy, 0.95);
        assert_eq!(healthy.tier, ContextPressureTier::Healthy);
        assert_eq!(healthy.util_pct, 149_999.0 / 200_000.0);
        assert!(!healthy.fork_suggested);

        let warn = classify_context_pressure(150_000, 200_000, ContextPressureTier::Healthy, 0.95);
        assert_eq!(warn.tier, ContextPressureTier::Warn);
        assert!(warn.fork_suggested);

        let still_warn =
            classify_context_pressure(140_000, 200_000, ContextPressureTier::Warn, 0.95);
        assert_eq!(still_warn.tier, ContextPressureTier::Warn);

        let recovered =
            classify_context_pressure(139_999, 200_000, ContextPressureTier::Warn, 0.95);
        assert_eq!(recovered.tier, ContextPressureTier::Healthy);

        let crit = classify_context_pressure(190_000, 200_000, ContextPressureTier::Warn, 0.95);
        assert_eq!(crit.tier, ContextPressureTier::Crit);
        assert!(crit.fork_suggested);
    }

    #[test]
    fn classify_context_pressure_clamps_invalid_inputs() {
        let no_window = classify_context_pressure(100_000, 0, ContextPressureTier::Warn, 0.95);
        assert_eq!(no_window.util_pct, 0.0);
        assert_eq!(no_window.tier, ContextPressureTier::Warn);

        let overflow =
            classify_context_pressure(300_000, 200_000, ContextPressureTier::Healthy, 0.95);
        assert_eq!(overflow.util_pct, 1.0);
        assert_eq!(overflow.tier, ContextPressureTier::Crit);
    }
}
