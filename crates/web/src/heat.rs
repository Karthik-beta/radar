const HOUR: i64 = 3_600;
const DAY: i64 = 86_400;

/// How long an item has waited, as the colour of its age (ADR 0003).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heat {
    Fresh,
    Warm,
    Hot,
}

impl Heat {
    /// The `data-heat` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Warm => "warm",
            Self::Hot => "hot",
        }
    }

    /// The token colour class for the age. Fresh ages stay `muted`.
    pub fn class(self) -> &'static str {
        match self {
            Self::Fresh => "text-muted",
            Self::Warm => "text-warm",
            Self::Hot => "text-hot",
        }
    }
}

/// Ages in seconds at which an item turns `Warm` and then `Hot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub warm: i64,
    pub hot: i64,
}

impl Thresholds {
    pub const YOUR_TURN: Self = Self {
        warm: DAY,
        hot: 3 * DAY,
    };
}

pub fn heat(age_secs: i64, thresholds: Thresholds) -> Heat {
    if age_secs >= thresholds.hot {
        Heat::Hot
    } else if age_secs >= thresholds.warm {
        Heat::Warm
    } else {
        Heat::Fresh
    }
}

/// The age in words, rounded down to its largest unit: "2 hours", "1 day".
pub fn age_words(age_secs: i64) -> String {
    let age = age_secs.max(0);
    let (n, unit) = if age >= DAY {
        (age / DAY, "day")
    } else if age >= HOUR {
        (age / HOUR, "hour")
    } else if age >= 60 {
        (age / 60, "minute")
    } else {
        return "less than a minute".to_owned();
    };
    let plural = if n == 1 { "" } else { "s" };
    format!("{n} {unit}{plural}")
}

/// The age as a row shows it: "4m", "2h", "3d".
pub fn age_short(age_secs: i64) -> String {
    let age = age_secs.max(0);
    if age >= DAY {
        format!("{}d", age / DAY)
    } else if age >= HOUR {
        format!("{}h", age / HOUR)
    } else {
        format!("{}m", age / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heat_turns_warm_at_one_day_and_hot_at_three() {
        let t = Thresholds::YOUR_TURN;
        let at = |h: i64, m: i64| heat(h * HOUR + m * 60, t);
        assert_eq!(at(23, 59), Heat::Fresh);
        assert_eq!(at(24, 0), Heat::Warm);
        assert_eq!(at(71, 59), Heat::Warm);
        assert_eq!(at(72, 0), Heat::Hot);
    }

    #[test]
    fn heat_age_words_round_down_to_the_largest_unit() {
        assert_eq!(age_words(2 * HOUR + 59 * 60), "2 hours");
        assert_eq!(age_words(DAY + 5 * HOUR), "1 day");
        assert_eq!(age_words(4 * DAY + 23 * HOUR), "4 days");
        assert_eq!(age_words(60), "1 minute");
        assert_eq!(age_words(-5), "less than a minute");
    }

    #[test]
    fn heat_age_short_uses_one_letter_units() {
        assert_eq!(age_short(5 * 60), "5m");
        assert_eq!(age_short(3 * HOUR), "3h");
        assert_eq!(age_short(73 * HOUR), "3d");
    }
}
