//! The three GitHub notification headers Radar reads from a forwarded email.
//! The parser never receives a body, so it cannot read one.

use crate::ids::{Login, RepoName};

/// A header longer than this is treated as missing, so a hostile header costs no CPU.
pub const MAX_HEADER_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmailHeaders<'a> {
    pub reason: Option<&'a str>,
    pub list_id: Option<&'a str>,
    pub sender: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailReason {
    Assign,
    Author,
    CiActivity,
    Comment,
    Mention,
    ReviewRequested,
    StateChange,
    Subscribed,
    TeamMention,
    Other,
}

impl EmailReason {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "assign" => Self::Assign,
            "author" => Self::Author,
            "ci_activity" => Self::CiActivity,
            "comment" => Self::Comment,
            "mention" => Self::Mention,
            "review_requested" => Self::ReviewRequested,
            "state_change" => Self::StateChange,
            "subscribed" => Self::Subscribed,
            "team_mention" => Self::TeamMention,
            _ => Self::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Assign => "assign",
            Self::Author => "author",
            Self::CiActivity => "ci_activity",
            Self::Comment => "comment",
            Self::Mention => "mention",
            Self::ReviewRequested => "review_requested",
            Self::StateChange => "state_change",
            Self::Subscribed => "subscribed",
            Self::TeamMention => "team_mention",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailRejected {
    NotGithub,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailTrigger {
    pub reason: EmailReason,
    pub repo: Option<RepoName>,
    pub sender: Option<Login>,
}

fn bounded(value: Option<&str>) -> Option<&str> {
    value.filter(|v| v.len() <= MAX_HEADER_BYTES)
}

impl EmailTrigger {
    pub fn from_headers(headers: &EmailHeaders<'_>) -> Result<Self, EmailRejected> {
        let reason = bounded(headers.reason)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or(EmailRejected::NotGithub)?;
        let repo = bounded(headers.list_id).and_then(repo_from_list_id);
        let sender = bounded(headers.sender).and_then(|v| Login::parse(v.trim()).ok());
        Ok(Self {
            reason: EmailReason::parse(reason),
            repo,
            sender,
        })
    }
}

/// GitHub sends `owner/repo <repo.owner.github.com>`.
fn repo_from_list_id(list_id: &str) -> Option<RepoName> {
    let (name, _) = list_id.split_once('<')?;
    RepoName::parse(name.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers<'a>(
        reason: Option<&'a str>,
        list_id: Option<&'a str>,
        sender: Option<&'a str>,
    ) -> EmailHeaders<'a> {
        EmailHeaders {
            reason,
            list_id,
            sender,
        }
    }

    const REAL_LIST_ID: &str = "octo/radar-demo <radar-demo.octo.github.com>";

    #[test]
    fn each_known_reason_maps() {
        let cases = [
            ("assign", EmailReason::Assign),
            ("author", EmailReason::Author),
            ("ci_activity", EmailReason::CiActivity),
            ("comment", EmailReason::Comment),
            ("mention", EmailReason::Mention),
            ("review_requested", EmailReason::ReviewRequested),
            ("state_change", EmailReason::StateChange),
            ("subscribed", EmailReason::Subscribed),
            ("team_mention", EmailReason::TeamMention),
            ("brand_new_reason", EmailReason::Other),
            ("  MENTION  ", EmailReason::Mention),
            ("Review_Requested", EmailReason::ReviewRequested),
        ];
        for (raw, want) in cases {
            let got = EmailTrigger::from_headers(&headers(Some(raw), None, None));
            assert_eq!(got.map(|t| t.reason), Ok(want), "{raw:?}");
        }
    }

    #[test]
    fn reason_names_round_trip() {
        for reason in [
            EmailReason::Assign,
            EmailReason::Author,
            EmailReason::CiActivity,
            EmailReason::Comment,
            EmailReason::Mention,
            EmailReason::ReviewRequested,
            EmailReason::StateChange,
            EmailReason::Subscribed,
            EmailReason::TeamMention,
            EmailReason::Other,
        ] {
            assert_eq!(EmailReason::parse(reason.as_str()), reason);
        }
    }

    #[test]
    fn missing_or_blank_reason_is_not_github() {
        for (name, reason) in [
            ("missing", None),
            ("empty", Some("")),
            ("spaces", Some("   ")),
        ] {
            assert_eq!(
                EmailTrigger::from_headers(&headers(reason, Some(REAL_LIST_ID), Some("octocat"))),
                Err(EmailRejected::NotGithub),
                "{name}"
            );
        }
    }

    #[test]
    fn list_id_cases() {
        let cases = [
            ("real github form", REAL_LIST_ID, Some("octo/radar-demo")),
            ("path traversal", "../../x <y>", None),
            ("no slash", "owner <y>", None),
            ("three segments", "a/b/c <y>", None),
            ("empty", "", None),
            ("no angle bracket", "octo/radar-demo", None),
        ];
        for (name, list_id, want) in cases {
            let got = EmailTrigger::from_headers(&headers(Some("mention"), Some(list_id), None))
                .map(|t| t.repo.map(|r| r.to_string()));
            assert_eq!(got, Ok(want.map(str::to_owned)), "{name}");
        }
    }

    #[test]
    fn missing_list_id_and_sender_are_none() {
        let got = EmailTrigger::from_headers(&headers(Some("mention"), None, None));
        assert_eq!(
            got,
            Ok(EmailTrigger {
                reason: EmailReason::Mention,
                repo: None,
                sender: None,
            })
        );
    }

    #[test]
    fn sender_must_be_a_login() {
        let cases = [
            ("maintainer", Some("maintainer")),
            (" maintainer ", Some("maintainer")),
            ("two words", None),
            ("zoë", None),
            ("", None),
        ];
        for (raw, want) in cases {
            let got = EmailTrigger::from_headers(&headers(Some("mention"), None, Some(raw)))
                .map(|t| t.sender.map(|l| l.to_string()));
            assert_eq!(got, Ok(want.map(str::to_owned)), "{raw:?}");
        }
    }

    #[test]
    fn header_length_limit_is_256_bytes() {
        let at_limit = "a".repeat(MAX_HEADER_BYTES);
        let over = "a".repeat(MAX_HEADER_BYTES + 1);

        let ok = EmailTrigger::from_headers(&headers(Some(&at_limit), None, None));
        assert_eq!(ok.map(|t| t.reason), Ok(EmailReason::Other));
        let long_reason = EmailTrigger::from_headers(&headers(Some(&over), None, None));
        assert_eq!(long_reason, Err(EmailRejected::NotGithub));

        let kept = EmailTrigger::from_headers(&headers(Some("mention"), None, Some(&at_limit)));
        assert_eq!(
            kept.map(|t| t.sender.map(|l| l.to_string())),
            Ok(Some(at_limit.clone()))
        );
        let dropped = EmailTrigger::from_headers(&headers(Some("mention"), None, Some(&over)));
        assert_eq!(dropped.map(|t| t.sender), Ok(None));

        let list_id = format!("octo/{} <x>", "r".repeat(MAX_HEADER_BYTES));
        let long_list = EmailTrigger::from_headers(&headers(Some("mention"), Some(&list_id), None));
        assert_eq!(long_list.map(|t| t.repo), Ok(None));
    }
}
