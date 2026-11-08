use std::fmt;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdError {
    EmptyLogin,
    BadLogin(String),
    BadRepoName(String),
    BadTimestamp(String),
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyLogin => f.write_str("login is empty"),
            Self::BadLogin(s) => write!(f, "invalid login `{s}`"),
            Self::BadRepoName(s) => write!(f, "invalid repo name `{s}`, expected owner/name"),
            Self::BadTimestamp(s) => {
                write!(f, "invalid timestamp `{s}`, expected YYYY-MM-DDTHH:MM:SSZ")
            }
        }
    }
}

impl std::error::Error for IdError {}

/// A GitHub login. GitHub treats logins case-insensitively, so equality and
/// hashing ignore ASCII case while the original spelling is kept for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Login(String);

impl Login {
    pub fn parse(s: &str) -> Result<Self, IdError> {
        if s.is_empty() {
            return Err(IdError::EmptyLogin);
        }
        let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '[' | ']');
        if !s.chars().all(allowed) {
            return Err(IdError::BadLogin(s.to_owned()));
        }
        Ok(Self(s.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq for Login {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for Login {}

impl Hash for Login {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for b in self.0.bytes() {
            state.write_u8(b.to_ascii_lowercase());
        }
        state.write_u8(0xff);
    }
}

impl fmt::Display for Login {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Login {
    type Error = IdError;
    fn try_from(s: String) -> Result<Self, IdError> {
        Self::parse(&s)
    }
}

impl From<Login> for String {
    fn from(login: Login) -> Self {
        login.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RepoName {
    owner: String,
    name: String,
}

impl RepoName {
    pub fn parse(s: &str) -> Result<Self, IdError> {
        let bad = || IdError::BadRepoName(s.to_owned());
        let (owner, name) = s.split_once('/').ok_or_else(bad)?;
        let owner_ok = !owner.is_empty()
            && owner
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
        let name_ok = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !owner_ok || !name_ok {
            return Err(bad());
        }
        Ok(Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for RepoName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

impl TryFrom<String> for RepoName {
    type Error = IdError;
    fn try_from(s: String) -> Result<Self, IdError> {
        Self::parse(&s)
    }
}

impl From<RepoName> for String {
    fn from(repo: RepoName) -> Self {
        repo.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Pr,
    Issue,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ItemRef {
    pub repo: RepoName,
    pub number: u32,
    pub kind: ItemKind,
}

impl fmt::Display for ItemRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.repo, self.number)
    }
}

/// Seconds since the Unix epoch, in UTC. Serialized in the
/// `YYYY-MM-DDTHH:MM:SSZ` form GitHub sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub i64);

const SECONDS_PER_DAY: i64 = 86_400;

impl Timestamp {
    pub fn parse_rfc3339(s: &str) -> Result<Self, IdError> {
        let bad = || IdError::BadTimestamp(s.to_owned());
        let b = s.as_bytes();
        if b.len() != 20
            || b[4] != b'-'
            || b[7] != b'-'
            || b[10] != b'T'
            || b[13] != b':'
            || b[16] != b':'
            || b[19] != b'Z'
        {
            return Err(bad());
        }
        let num = |range: std::ops::Range<usize>| -> Result<i64, IdError> {
            b[range].iter().try_fold(0i64, |acc, &d| {
                if d.is_ascii_digit() {
                    Ok(acc * 10 + i64::from(d - b'0'))
                } else {
                    Err(bad())
                }
            })
        };
        let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
        let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
        if !(1..=12).contains(&month)
            || day < 1
            || day > days_in_month(year, month)
            || hour > 23
            || minute > 59
            || second > 59
        {
            return Err(bad());
        }
        let days = days_from_civil(year, month, day);
        Ok(Self(
            days * SECONDS_PER_DAY + hour * 3600 + minute * 60 + second,
        ))
    }
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// Howard Hinnant's days_from_civil and civil_from_days, proleptic Gregorian.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let days = self.0.div_euclid(SECONDS_PER_DAY);
        let secs = self.0.rem_euclid(SECONDS_PER_DAY);
        let (year, month, day) = civil_from_days(days);
        write!(
            f,
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
            secs / 3600,
            secs % 3600 / 60,
            secs % 60
        )
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse_rfc3339(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn login_compares_case_insensitively() {
        let upper = Login::parse("KARTHIK-beta").unwrap();
        let lower = Login::parse("karthik-beta").unwrap();
        assert_eq!(upper, lower);
        assert_eq!(upper.to_string(), "KARTHIK-beta");
        let set: HashSet<Login> = [upper, lower].into_iter().collect();
        assert_eq!(set.len(), 1);
        assert_ne!(
            Login::parse("karthik").unwrap(),
            Login::parse("karthik-beta").unwrap()
        );
        assert_eq!(Login::parse(""), Err(IdError::EmptyLogin));
        assert!(Login::parse("has space").is_err());
    }

    #[test]
    fn repo_name_rejects_missing_owner() {
        for bad in ["/radar", "radar", "karthik-beta/", "a/b/c", "a b/c", ""] {
            assert!(RepoName::parse(bad).is_err(), "{bad} should be rejected");
        }
        let repo = RepoName::parse("Karthik-beta/radar.rs").unwrap();
        assert_eq!(repo.owner(), "Karthik-beta");
        assert_eq!(repo.name(), "radar.rs");
    }

    #[test]
    fn repo_name_accepts_a_managed_user_owner() {
        let repo = RepoName::parse("mona-cat_octo/radar").unwrap();
        assert_eq!(repo.owner(), "mona-cat_octo");
    }

    #[test]
    fn item_ref_displays_as_owner_name_hash_number() {
        let item_ref = ItemRef {
            repo: RepoName::parse("karthik-beta/radar").unwrap(),
            number: 123,
            kind: ItemKind::Pr,
        };
        assert_eq!(item_ref.to_string(), "karthik-beta/radar#123");
        let json = serde_json::to_string(&item_ref).unwrap();
        assert_eq!(
            json,
            r#"{"repo":"karthik-beta/radar","number":123,"kind":"pr"}"#
        );
        assert_eq!(serde_json::from_str::<ItemRef>(&json).unwrap(), item_ref);
    }

    #[test]
    fn timestamp_parses_github_format_and_rejects_garbage() {
        let cases = [
            ("1970-01-01T00:00:00Z", 0),
            ("2026-11-06T12:34:56Z", 1_793_968_496),
            ("2024-02-29T23:59:59Z", 1_709_251_199),
            ("1969-12-31T23:59:59Z", -1),
        ];
        for (text, epoch) in cases {
            let ts = Timestamp::parse_rfc3339(text).unwrap();
            assert_eq!(ts, Timestamp(epoch), "{text}");
            assert_eq!(ts.to_string(), text);
        }
        for bad in [
            "yesterday",
            "",
            "2026-11-06",
            "2026-11-06T12:34:56",
            "2026-11-06T12:34:56+00:00",
            "2026-11-06T12:34:56.000Z",
            "2026-13-01T00:00:00Z",
            "2025-02-29T00:00:00Z",
            "2026-11-06T24:00:00Z",
            "2026-11-06T12:60:00Z",
            "2026-11-06T12:00:60Z",
            "2026-11-06 12:34:56Z",
            "+026-11-06T12:34:56Z",
        ] {
            assert!(
                Timestamp::parse_rfc3339(bad).is_err(),
                "{bad} should be rejected"
            );
        }
        let json = serde_json::to_string(&Timestamp(0)).unwrap();
        assert_eq!(json, r#""1970-01-01T00:00:00Z""#);
        let err = serde_json::from_str::<Timestamp>(r#""yesterday""#).unwrap_err();
        assert!(err.to_string().contains("yesterday"));
    }
}
