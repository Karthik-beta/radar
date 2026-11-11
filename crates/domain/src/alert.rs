//! Which events reach my phone, and what the message says (F13).

use crate::event::{Event, EventKind};
use crate::ids::{Login, RepoName, Timestamp};
use crate::item::ItemState;
use crate::turn::{Reason, Turn};

/// An event older than this is not sent, so switching alerts on never floods the phone.
pub const MAX_AGE_SECS: i64 = 6 * 60 * 60;

/// Failed sends stop after this many attempts.
pub const MAX_ATTEMPTS: u32 = 3;

const MAX_TITLE_CHARS: usize = 200;

#[derive(Clone, PartialEq, Eq)]
pub struct TelegramConfig {
    pub token: String,
    pub chat_id: String,
}

impl std::fmt::Debug for TelegramConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramConfig")
            .field("token", &"<redacted>")
            .field("chat_id", &self.chat_id)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertsMode {
    Off,
    On(TelegramConfig),
}

impl AlertsMode {
    /// Both secrets must be present and non-empty, otherwise alerts stay off.
    pub fn from_secrets(token: Option<&str>, chat_id: Option<&str>) -> Self {
        let present = |value: Option<&str>| {
            value
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        match (present(token), present(chat_id)) {
            (Some(token), Some(chat_id)) => Self::On(TelegramConfig { token, chat_id }),
            _ => Self::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    WrongReason,
    NotMyTurn,
    BotActor,
    TooOld,
}

impl SkipReason {
    /// True when no later change can turn the skip into a send. A turn that
    /// flips back or a snooze that ends can still make `NotMyTurn` a send.
    pub fn is_permanent(self) -> bool {
        !matches!(self, Self::NotMyTurn)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertDecision {
    Send,
    Skip(SkipReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertState {
    Sent,
    Failed { attempts: u32 },
}

/// What `decide` needs to know about the item an event happened on.
/// `visible` is false for a dismissed or still-snoozed item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertItem {
    pub number: u32,
    pub title: String,
    pub url: String,
    pub turn: Turn,
    pub state: ItemState,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertReason {
    Mentioned,
    ReviewRequested,
}

impl AlertReason {
    fn of(kind: EventKind) -> Option<Self> {
        match kind {
            EventKind::Mentioned => Some(Self::Mentioned),
            EventKind::ReviewRequested => Some(Self::ReviewRequested),
            _ => None,
        }
    }

    fn turn_reason(self) -> Reason {
        match self {
            Self::Mentioned => Reason::Mentioned,
            Self::ReviewRequested => Reason::ReviewRequested,
        }
    }
}

pub fn decide(event: &Event, item: &AlertItem, now: Timestamp) -> AlertDecision {
    let Some(reason) = AlertReason::of(event.kind) else {
        return AlertDecision::Skip(SkipReason::WrongReason);
    };
    let Some(actor) = event.actor.as_ref() else {
        return AlertDecision::Skip(SkipReason::WrongReason);
    };
    if actor.is_bot {
        return AlertDecision::Skip(SkipReason::BotActor);
    }
    let open = matches!(item.state, ItemState::Open | ItemState::Draft);
    if !open || !item.visible || item.turn != Turn::Me(reason.turn_reason()) {
        return AlertDecision::Skip(SkipReason::NotMyTurn);
    }
    if now.0.saturating_sub(event.occurred_at.0) > MAX_AGE_SECS {
        return AlertDecision::Skip(SkipReason::TooOld);
    }
    AlertDecision::Send
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub reason: AlertReason,
    pub repo: RepoName,
    pub number: u32,
    pub actor: Login,
    pub title: String,
    pub url: String,
}

impl Alert {
    /// `None` for an event that never alerts.
    pub fn new(event: &Event, item: &AlertItem) -> Option<Self> {
        Some(Self {
            reason: AlertReason::of(event.kind)?,
            repo: event.repo.clone(),
            number: item.number,
            actor: event.actor.as_ref()?.login.clone(),
            title: item.title.clone(),
            url: item.url.clone(),
        })
    }
}

/// Plain text, at most four lines, metadata only. The message carries no
/// `parse_mode`, so markup in a title arrives as inert text.
pub fn alert_text(alert: &Alert) -> String {
    let what = match alert.reason {
        AlertReason::Mentioned => "Mention",
        AlertReason::ReviewRequested => "Review requested",
    };
    let title: String = alert
        .title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_TITLE_CHARS)
        .collect();
    format!(
        "{what} in {}#{}\nFrom {}\n{title}\n{}",
        alert.repo, alert.number, alert.actor, alert.url
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventActor, EventId};

    const NOW: Timestamp = Timestamp(1_800_000_000);

    fn event(kind: EventKind, bot: bool, age: i64) -> Event {
        Event {
            id: EventId::for_node("E_1"),
            item: "PR_1".to_owned(),
            repo: RepoName::parse("owner/repo").unwrap(),
            kind,
            actor: Some(EventActor {
                login: Login::parse("alice").unwrap(),
                is_me: false,
                is_bot: bot,
            }),
            occurred_at: Timestamp(NOW.0 - age),
        }
    }

    fn item(turn: Turn) -> AlertItem {
        AlertItem {
            number: 7,
            title: "Fix *the* <b>thing</b> `x` _y_".to_owned(),
            url: "https://github.com/owner/repo/pull/7".to_owned(),
            turn,
            state: ItemState::Open,
            visible: true,
        }
    }

    const MENTION: Turn = Turn::Me(Reason::Mentioned);
    const REVIEW: Turn = Turn::Me(Reason::ReviewRequested);

    #[test]
    fn fresh_mention_and_review_request_on_my_turn_send() {
        assert_eq!(
            decide(&event(EventKind::Mentioned, false, 60), &item(MENTION), NOW),
            AlertDecision::Send
        );
        assert_eq!(
            decide(
                &event(EventKind::ReviewRequested, false, 60),
                &item(REVIEW),
                NOW
            ),
            AlertDecision::Send
        );
    }

    #[test]
    fn other_event_kinds_are_the_wrong_reason() {
        for kind in [
            EventKind::CiFailed,
            EventKind::Commented,
            EventKind::Assigned,
        ] {
            assert_eq!(
                decide(&event(kind, false, 60), &item(MENTION), NOW),
                AlertDecision::Skip(SkipReason::WrongReason),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_bot_mention_is_skipped() {
        assert_eq!(
            decide(&event(EventKind::Mentioned, true, 60), &item(MENTION), NOW),
            AlertDecision::Skip(SkipReason::BotActor)
        );
    }

    #[test]
    fn a_hidden_closed_or_other_turn_item_is_not_my_turn() {
        let fresh = event(EventKind::Mentioned, false, 60);
        let mut dismissed = item(MENTION);
        dismissed.visible = false;
        let mut closed = item(MENTION);
        closed.state = ItemState::Closed;
        for item in [dismissed, closed, item(Turn::Them), item(REVIEW)] {
            assert_eq!(
                decide(&fresh, &item, NOW),
                AlertDecision::Skip(SkipReason::NotMyTurn)
            );
        }
    }

    #[test]
    fn six_hours_sends_and_one_second_more_is_too_old() {
        let at = |age| {
            decide(
                &event(EventKind::Mentioned, false, age),
                &item(MENTION),
                NOW,
            )
        };
        assert_eq!(at(MAX_AGE_SECS), AlertDecision::Send);
        assert_eq!(
            at(MAX_AGE_SECS + 1),
            AlertDecision::Skip(SkipReason::TooOld)
        );
    }

    #[test]
    fn mode_is_on_only_with_both_secrets() {
        let off = AlertsMode::Off;
        assert_eq!(AlertsMode::from_secrets(None, None), off);
        assert_eq!(AlertsMode::from_secrets(Some("t"), None), off);
        assert_eq!(AlertsMode::from_secrets(None, Some("1")), off);
        assert_eq!(AlertsMode::from_secrets(Some(""), Some("1")), off);
        assert_eq!(AlertsMode::from_secrets(Some("t"), Some("")), off);
        assert_eq!(
            AlertsMode::from_secrets(Some("t"), Some("1")),
            AlertsMode::On(TelegramConfig {
                token: "t".into(),
                chat_id: "1".into()
            })
        );
    }

    #[test]
    fn the_token_is_redacted_in_debug_output() {
        let mode = AlertsMode::from_secrets(Some("secret-token"), Some("42"));
        assert!(!format!("{mode:?}").contains("secret-token"));
    }

    #[test]
    fn text_keeps_markup_and_never_holds_the_token() {
        let ev = event(EventKind::Mentioned, false, 60);
        let it = item(MENTION);
        let alert = Alert::new(&ev, &it).unwrap();
        let text = alert_text(&alert);
        assert!(text.contains("Fix *the* <b>thing</b> `x` _y_"), "{text}");
        assert!(text.contains("owner/repo#7"));
        assert!(text.contains("https://github.com/owner/repo/pull/7"));
        assert!(text.lines().count() <= 4);
        assert!(!text.contains("secret-token"));
    }

    #[test]
    fn a_multi_line_title_stays_on_one_line() {
        let ev = event(EventKind::ReviewRequested, false, 60);
        let mut it = item(REVIEW);
        it.title = "a\nb\r\nc".to_owned();
        let text = alert_text(&Alert::new(&ev, &it).unwrap());
        assert_eq!(text.lines().count(), 4, "{text}");
    }

    #[test]
    fn no_alert_for_an_event_that_never_alerts() {
        assert!(Alert::new(&event(EventKind::CiFailed, false, 1), &item(MENTION)).is_none());
    }
}
