use domain::api::{Budget, Freshness, ScopeState, SourceFreshness};
use domain::sync_status::{SyncKind, SyncStatus};

pub fn freshness(status: SyncStatus) -> Freshness {
    let sources = SyncKind::ALL
        .into_iter()
        .map(|source| SourceFreshness {
            source,
            last_ok_at: status.last_ok_by_kind.get(&source).copied(),
            pending: status.pending_by_kind.get(&source).copied().unwrap_or(0),
        })
        .collect();
    Freshness {
        sources,
        budget: Budget {
            limit: status.rate.limit,
            remaining: status.rate.remaining,
            reset_at: status.rate.reset,
        },
        dead: status.dead,
        scope: ScopeState::parse(status.scopes.as_deref()),
        next_poll_at: status.next_poll_at,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use domain::ids::Timestamp;
    use domain::sync_status::{DeadWork, Rate};

    use super::*;

    #[test]
    fn every_source_appears_with_its_last_success_and_queue() {
        let status = SyncStatus {
            pending_by_kind: BTreeMap::from([(SyncKind::Enrich, 2)]),
            last_ok_by_kind: BTreeMap::from([(SyncKind::PollNotifications, Timestamp(900))]),
            next_poll_at: Some(Timestamp(960)),
            rate: Rate {
                limit: Some(5000),
                remaining: Some(4990),
                reset: Some(Timestamp(4000)),
            },
            dead: vec![DeadWork {
                kind: SyncKind::Enrich,
                key: "abc".into(),
                attempts: 3,
                since: Timestamp(800),
            }],
            scopes: Some("notifications,read:user".into()),
            ..SyncStatus::default()
        };
        let out = freshness(status);
        let sources: Vec<_> = out
            .sources
            .iter()
            .map(|s| (s.source, s.last_ok_at, s.pending))
            .collect();
        assert_eq!(
            sources,
            [
                (SyncKind::PollNotifications, Some(Timestamp(900)), 0),
                (SyncKind::Enrich, None, 2),
                (SyncKind::BackfillPage, None, 0),
                (SyncKind::Rollup, None, 0),
            ]
        );
        assert_eq!(out.scope, ScopeState::Ok);
        assert_eq!(out.budget.remaining, Some(4990));
        assert_eq!(out.dead[0].attempts, 3);
        assert_eq!(out.next_poll_at, Some(Timestamp(960)));
    }

    #[test]
    fn scope_reads_what_github_last_sent() {
        let with = |scopes: Option<&str>| {
            freshness(SyncStatus {
                scopes: scopes.map(str::to_owned),
                ..SyncStatus::default()
            })
            .scope
        };
        assert_eq!(with(None), ScopeState::Unknown);
        assert_eq!(with(Some("read:user")), ScopeState::MissingNotifications);
        assert_eq!(with(Some("notifications")), ScopeState::Ok);
    }
}
