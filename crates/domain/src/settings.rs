use std::fmt;

use serde::{Deserialize, Serialize};

/// When a wait on someone else turns `warm` and then `hot` (F4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawThresholds")]
pub struct NudgeThresholds {
    warm_days: u32,
    hot_days: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NudgeError {
    Zero,
    HotNotAfterWarm { warm_days: u32, hot_days: u32 },
}

impl fmt::Display for NudgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => f.write_str("nudge thresholds must be at least one day"),
            Self::HotNotAfterWarm {
                warm_days,
                hot_days,
            } => write!(
                f,
                "hot_days ({hot_days}) must be greater than warm_days ({warm_days})"
            ),
        }
    }
}

impl std::error::Error for NudgeError {}

impl NudgeThresholds {
    pub fn new(warm_days: u32, hot_days: u32) -> Result<Self, NudgeError> {
        if warm_days == 0 || hot_days == 0 {
            return Err(NudgeError::Zero);
        }
        if hot_days <= warm_days {
            return Err(NudgeError::HotNotAfterWarm {
                warm_days,
                hot_days,
            });
        }
        Ok(Self {
            warm_days,
            hot_days,
        })
    }

    pub fn warm_days(self) -> u32 {
        self.warm_days
    }

    pub fn hot_days(self) -> u32 {
        self.hot_days
    }
}

impl Default for NudgeThresholds {
    fn default() -> Self {
        Self {
            warm_days: 7,
            hot_days: 14,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawThresholds {
    warm_days: u32,
    hot_days: u32,
}

impl TryFrom<RawThresholds> for NudgeThresholds {
    type Error = NudgeError;

    fn try_from(raw: RawThresholds) -> Result<Self, NudgeError> {
        Self::new(raw.warm_days, raw.hot_days)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_seven_and_fourteen_days() {
        let nudge = NudgeThresholds::default();
        assert_eq!((nudge.warm_days(), nudge.hot_days()), (7, 14));
        assert_eq!(NudgeThresholds::new(7, 14), Ok(nudge));
    }

    #[test]
    fn rejects_zero_and_hot_at_or_before_warm() {
        assert_eq!(NudgeThresholds::new(0, 5), Err(NudgeError::Zero));
        assert!(matches!(
            NudgeThresholds::new(7, 7),
            Err(NudgeError::HotNotAfterWarm { .. })
        ));
        assert!(NudgeThresholds::new(9, 3).is_err());
        assert!(NudgeThresholds::new(3, 5).is_ok());
    }

    #[test]
    fn json_goes_through_the_same_check() {
        let json = serde_json::to_string(&NudgeThresholds::default()).unwrap();
        assert_eq!(json, r#"{"warm_days":7,"hot_days":14}"#);
        assert!(
            serde_json::from_str::<NudgeThresholds>(r#"{"warm_days":7,"hot_days":7}"#).is_err()
        );
    }
}
