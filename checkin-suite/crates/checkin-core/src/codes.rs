//! Platform business codes → CheckinOutcome mapping.
//! Codes are from the Python对照 suite; pending live verification.

use crate::types::CheckinOutcome;

pub const WB_OK: i64 = 0;
pub const WB_ALREADY: i64 = 10001;
pub const WB_SESSION_INVALID: i64 = 12153;

pub const TRAE_OK: i64 = 0;
pub const TRAE_ALREADY: i64 = 9095;
pub const TRAE_RATE_LIMIT: i64 = 9074;
pub const TRAE_BAD_DEVICE: i64 = 9004;

/// Map WorkBuddy business code to outcome.
pub fn map_workbuddy_code(
    code: i64,
    credits_delta: Option<i64>,
    streak_days: Option<i32>,
    balance: Option<i64>,
    message: impl Into<String>,
) -> CheckinOutcome {
    let message = message.into();
    match code {
        WB_OK => CheckinOutcome::Success {
            credits_delta,
            streak_days,
            balance,
            message,
            raw_code: Some(code),
        },
        WB_ALREADY => CheckinOutcome::AlreadyDone {
            credits_delta: None,
            streak_days,
            balance,
            message,
            raw_code: Some(code),
        },
        WB_SESSION_INVALID => CheckinOutcome::Disabled { reason: message },
        _ => CheckinOutcome::Failed {
            code: Some(code),
            message,
            retryable: false,
        },
    }
}

/// Map Trae CN business code to outcome.
pub fn map_trae_code(
    code: i64,
    credits_delta: Option<i64>,
    streak_days: Option<i32>,
    balance: Option<i64>,
    message: impl Into<String>,
) -> CheckinOutcome {
    let message = message.into();
    match code {
        TRAE_OK => CheckinOutcome::Success {
            credits_delta,
            streak_days,
            balance,
            message,
            raw_code: Some(code),
        },
        TRAE_ALREADY => CheckinOutcome::AlreadyDone {
            credits_delta: None,
            streak_days,
            balance,
            message,
            raw_code: Some(code),
        },
        TRAE_RATE_LIMIT => CheckinOutcome::Failed {
            code: Some(code),
            message,
            retryable: true,
        },
        TRAE_BAD_DEVICE => CheckinOutcome::Failed {
            code: Some(code),
            message,
            retryable: false,
        },
        1001 | 1002 | 401 | 403 => CheckinOutcome::Failed {
            code: Some(code),
            message,
            retryable: false,
        },
        _ => CheckinOutcome::Failed {
            code: Some(code),
            message,
            retryable: false,
        },
    }
}

/// True if code means "already checked in today".
pub fn is_already_done_code(platform: &str, code: i64) -> bool {
    match platform {
        "workbuddy" => code == WB_ALREADY,
        "trae" => code == TRAE_ALREADY,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wb_10001_is_already_done() {
        let o = map_workbuddy_code(WB_ALREADY, Some(10), Some(3), Some(100), "已签");
        assert_eq!(o.kind_str(), "AlreadyDone");
        assert_eq!(o.raw_code(), Some(10001));
        assert!(o.credits_delta().is_none());
    }

    #[test]
    fn trae_9095_is_already_done() {
        let o = map_trae_code(TRAE_ALREADY, None, None, None, "already");
        assert_eq!(o.kind_str(), "AlreadyDone");
        assert!(is_already_done_code("trae", 9095));
    }
}
