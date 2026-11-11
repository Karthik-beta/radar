//! Phone alerts through Telegram (F13). `dispatch` is the `SendAlerts` unit: it
//! reads events Enrich already wrote, decides, sends, and records each result.

mod telegram;

use domain::alert::{
    alert_text, decide, Alert, AlertDecision, AlertsMode, SkipReason, MAX_AGE_SECS,
};
use domain::event::EventId;
use domain::ids::Timestamp;
use worker::{console_log, D1Database, Env};

use crate::auth::keys::read_setting;
use crate::db::alerts::{self, Candidate, CandidateRow};
use crate::db::run;
use crate::sync::scheduler::UnitOutcome;
use crate::sync::work::WorkKind;

const TOKEN_VAR: &str = "TELEGRAM_BOT_TOKEN";
const CHAT_VAR: &str = "TELEGRAM_CHAT_ID";
const API_BASE_VAR: &str = "TELEGRAM_API_BASE";
const DEFAULT_API_BASE: &str = "https://api.telegram.org";

/// One dispatch sends at most this many, so one alarm stays short.
pub const BATCH: usize = 5;

/// A message to send for one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub event: EventId,
    pub text: String,
}

/// The messages this dispatch sends, in event order, and whether more wait.
/// A skipped candidate is left without a row and returned for the caller to log.
pub fn plan(
    candidates: &[Candidate],
    now: Timestamp,
) -> (Vec<Outgoing>, bool, Vec<(EventId, SkipReason)>) {
    let mut sends = Vec::new();
    let mut skipped = Vec::new();
    for candidate in candidates {
        match decide(&candidate.event, &candidate.item, now) {
            AlertDecision::Send => {
                if let Some(alert) = Alert::new(&candidate.event, &candidate.item) {
                    sends.push(Outgoing {
                        event: candidate.event.id.clone(),
                        text: alert_text(&alert),
                    });
                }
            }
            AlertDecision::Skip(why) => skipped.push((candidate.event.id.clone(), why)),
        }
    }
    let more = sends.len() > BATCH;
    sends.truncate(BATCH);
    (sends, more, skipped)
}

pub async fn dispatch(db: &D1Database, env: &Env, now: Timestamp) -> UnitOutcome {
    let retry = |error: String| UnitOutcome::Retry { error };
    let mode = AlertsMode::from_secrets(
        read_setting(env, TOKEN_VAR).as_deref(),
        read_setting(env, CHAT_VAR).as_deref(),
    );
    let AlertsMode::On(config) = mode else {
        console_log!("alerts off");
        return UnitOutcome::Done { follow_ups: vec![] };
    };
    let base = match read_setting(env, API_BASE_VAR).filter(|v| !v.trim().is_empty()) {
        None => DEFAULT_API_BASE.to_owned(),
        Some(raw) => match telegram::allowed_base(&raw) {
            Some(base) => base,
            None => {
                console_log!("alerts off: ignored TELEGRAM_API_BASE");
                return UnitOutcome::Done { follow_ups: vec![] };
            }
        },
    };

    let since = Timestamp(now.0.saturating_sub(MAX_AGE_SECS));
    let rows = match alerts::candidates(since).prepare(db) {
        Ok(stmt) => stmt.all().await.and_then(|r| r.results::<CandidateRow>()),
        Err(err) => Err(err),
    };
    let candidates = match rows {
        Ok(rows) => rows
            .into_iter()
            .map(|row| row.into_candidate(now))
            .collect::<Result<Vec<_>, _>>(),
        Err(err) => return retry(format!("d1 read: {err}")),
    };
    let candidates = match candidates {
        Ok(candidates) => candidates,
        Err(err) => return retry(format!("alert row: {err}")),
    };

    let (sends, more, skipped) = plan(&candidates, now);
    for (id, why) in &skipped {
        console_log!("alert event={} outcome=skipped why={why:?}", id.as_str());
    }
    let mut failed = None;
    for outgoing in &sends {
        let sent = telegram::send(&base, &config, &outgoing.text).await;
        let outcome = if sent.is_ok() { "sent" } else { "failed" };
        console_log!(
            "alert event={} outcome={outcome} path=/bot<redacted>/sendMessage",
            outgoing.event.as_str()
        );
        let record = match sent {
            Ok(()) => alerts::record_sent(&outgoing.event, now),
            Err(err) => {
                console_log!("alert event={} error={err}", outgoing.event.as_str());
                failed.get_or_insert_with(|| err.to_string());
                alerts::record_failed(&outgoing.event, now)
            }
        };
        if let Err(err) = run(db, vec![record]).await {
            return retry(format!("d1 write: {err}"));
        }
    }
    after_sends(failed, more)
}

/// A failed send returns `Retry` after its row is recorded, so the scheduler
/// backs off and re-admits the row while attempts remain. Otherwise the alert
/// would wait for the next Enrich run and could outlive `MAX_AGE_SECS`.
pub fn after_sends(failed: Option<String>, more: bool) -> UnitOutcome {
    match failed {
        Some(error) => UnitOutcome::Retry {
            error: format!("telegram send: {error}"),
        },
        None if more => UnitOutcome::Done {
            follow_ups: vec![WorkKind::SendAlerts],
        },
        None => UnitOutcome::Done { follow_ups: vec![] },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::alert::AlertItem;
    use domain::event::{Event, EventActor, EventKind};
    use domain::ids::{Login, RepoName};
    use domain::item::ItemState;
    use domain::turn::{Reason, Turn};

    const NOW: Timestamp = Timestamp(1_800_000_000);

    fn candidate(n: i64, bot: bool, age: i64) -> Candidate {
        Candidate {
            event: Event {
                id: EventId::for_node(&format!("E{n}")),
                item: format!("PR_{n}"),
                repo: RepoName::parse("o/r").expect("repo"),
                kind: EventKind::Mentioned,
                actor: Some(EventActor {
                    login: Login::parse("alice").expect("login"),
                    is_me: false,
                    is_bot: bot,
                }),
                occurred_at: Timestamp(NOW.0 - age),
            },
            item: AlertItem {
                number: 1,
                title: "t".into(),
                url: "u".into(),
                turn: Turn::Me(Reason::Mentioned),
                state: ItemState::Open,
                visible: true,
            },
        }
    }

    #[test]
    fn a_failed_send_retries_and_a_clean_batch_follows_up_only_when_more_wait() {
        assert_eq!(
            after_sends(Some("429".into()), true),
            UnitOutcome::Retry {
                error: "telegram send: 429".into()
            }
        );
        assert_eq!(
            after_sends(None, true),
            UnitOutcome::Done {
                follow_ups: vec![WorkKind::SendAlerts]
            }
        );
        assert_eq!(
            after_sends(None, false),
            UnitOutcome::Done { follow_ups: vec![] }
        );
    }

    #[test]
    fn twelve_fresh_mentions_send_five_and_leave_more() {
        let all: Vec<_> = (0..12).map(|n| candidate(n, false, 60)).collect();
        let (sends, more, _) = plan(&all, NOW);
        assert_eq!(sends.len(), BATCH);
        assert!(more);
        assert_eq!(sends[0].event.as_str(), "E0");
    }

    #[test]
    fn skipped_candidates_do_not_count_toward_the_batch() {
        let mut all: Vec<_> = (0..8).map(|n| candidate(n, true, 60)).collect();
        all.push(candidate(9, false, 60));
        all.push(candidate(10, false, MAX_AGE_SECS + 1));
        let (sends, more, _) = plan(&all, NOW);
        assert_eq!(sends.len(), 1);
        assert!(!more);
    }
}
