use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::Login;

/// GitHub's numeric account id. Logins can change hands, ids never do, so the
/// allowlist compares ids (A2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GithubUserId(pub u64);

impl GithubUserId {
    pub fn parse(raw: &str) -> Option<Self> {
        let digits = raw.trim();
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok().filter(|id| *id > 0).map(Self)
    }
}

impl fmt::Display for GithubUserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The signed-in account, as `GET /api/me` returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Me {
    pub login: Login,
    pub user_id: GithubUserId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_id_parses_decimal_digits_only() {
        assert_eq!(GithubUserId::parse("1001"), Some(GithubUserId(1001)));
        assert_eq!(GithubUserId::parse(" 1001\n"), Some(GithubUserId(1001)));
        assert_eq!(GithubUserId::parse("01001"), Some(GithubUserId(1001)));
        for bad in [
            "",
            "0",
            "-1",
            "+1",
            "1e3",
            "10.0",
            "abc",
            "18446744073709551616",
        ] {
            assert_eq!(GithubUserId::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn me_round_trips_with_a_numeric_id() {
        let me = Me {
            login: Login::parse("karthik").unwrap(),
            user_id: GithubUserId(1001),
        };
        let json = serde_json::to_string(&me).unwrap();
        assert_eq!(json, r#"{"login":"karthik","user_id":1001}"#);
        assert_eq!(serde_json::from_str::<Me>(&json).unwrap(), me);
    }

    #[test]
    fn me_rejects_a_string_id_and_extra_fields() {
        assert!(serde_json::from_str::<Me>(r#"{"login":"k","user_id":"1001"}"#).is_err());
        assert!(serde_json::from_str::<Me>(r#"{"login":"k","user_id":1,"x":1}"#).is_err());
    }
}
