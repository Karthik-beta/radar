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

/// The `settings` row as D1 returns it, before validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsRow {
    pub warm_days: i64,
    pub hot_days: i64,
}

/// Why `NudgeThresholds::from_row` returned the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsFallback {
    Missing,
    OutOfRange(SettingsRow),
    Rejected(NudgeError),
}

impl fmt::Display for SettingsFallback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("settings row is missing, using 7 and 14 days"),
            Self::OutOfRange(row) => write!(
                f,
                "settings row ({}, {}) is out of range, using 7 and 14 days",
                row.warm_days, row.hot_days
            ),
            Self::Rejected(err) => {
                write!(f, "settings row is invalid ({err}), using 7 and 14 days")
            }
        }
    }
}

impl NudgeThresholds {
    /// The stored pair, or why it cannot be used.
    pub fn try_from_row(row: Option<SettingsRow>) -> Result<Self, SettingsFallback> {
        let row = row.ok_or(SettingsFallback::Missing)?;
        let days = |n: i64| u32::try_from(n).ok();
        let (Some(warm), Some(hot)) = (days(row.warm_days), days(row.hot_days)) else {
            return Err(SettingsFallback::OutOfRange(row));
        };
        Self::new(warm, hot).map_err(SettingsFallback::Rejected)
    }

    /// The stored pair when valid, else the default. It never fails (F4).
    pub fn from_row(row: Option<SettingsRow>) -> Self {
        Self::try_from_row(row).unwrap_or_default()
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
    fn from_row_keeps_a_valid_pair_and_defaults_otherwise() {
        let row = |warm_days, hot_days| {
            Some(SettingsRow {
                warm_days,
                hot_days,
            })
        };
        let stored = NudgeThresholds::from_row(row(3, 5));
        assert_eq!((stored.warm_days(), stored.hot_days()), (3, 5));
        assert_eq!(NudgeThresholds::from_row(None), NudgeThresholds::default());
        assert_eq!(
            NudgeThresholds::from_row(row(9, 9)),
            NudgeThresholds::default()
        );
        assert_eq!(
            NudgeThresholds::try_from_row(row(-1, 5)),
            Err(SettingsFallback::OutOfRange(SettingsRow {
                warm_days: -1,
                hot_days: 5
            }))
        );
        assert_eq!(
            NudgeThresholds::try_from_row(None),
            Err(SettingsFallback::Missing)
        );
        assert!(matches!(
            NudgeThresholds::try_from_row(row(0, 5)),
            Err(SettingsFallback::Rejected(NudgeError::Zero))
        ));
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
