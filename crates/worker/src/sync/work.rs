//! One unit of Syncer work and how it maps to a row of the `work` table.

use std::fmt;

use domain::ids::{RepoName, Timestamp};
use domain::rollup::WeekStart;
use domain::sync_status::SyncKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::backfill::{valid_cursor, BackfillCursor};
use super::config::{ENRICH_BATCH, IMPORT_PAGE};
use super::history::HistoryKey;
use crate::rollup::RollupKey;

const MAIN_KEY: &str = "main";
const MAX_NODE_ID_LEN: usize = 128;
const SECONDS_PER_DAY: i64 = 86_400;
const START: &str = "start";
const IMPORT_PREFIX: &str = "import";
const REPOS_PREFIX: &str = "repos";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkError {
    BadKind(String),
    BadKey { kind: SyncKind, key: String },
    BadNodeId(String),
    BatchSize(usize),
    MissingPayload,
}

impl fmt::Display for WorkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadKind(kind) => write!(f, "unknown work kind {kind:?}"),
            Self::BadKey { kind, key } => write!(f, "bad key {key:?} for {}", kind.as_str()),
            Self::BadNodeId(id) => write!(f, "bad node id {id:?}"),
            Self::BatchSize(n) => {
                write!(f, "an enrich batch holds 1 to {ENRICH_BATCH} ids, not {n}")
            }
            Self::MissingPayload => f.write_str("enrich row has no node ids"),
        }
    }
}

/// A GraphQL node id, such as `PR_kwDOAbc123`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(String);

impl NodeId {
    pub fn parse(raw: &str) -> Result<Self, WorkError> {
        let ok = !raw.is_empty()
            && raw.len() <= MAX_NODE_ID_LEN
            && raw
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'='));
        if ok {
            Ok(Self(raw.to_owned()))
        } else {
            Err(WorkError::BadNodeId(raw.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// At most `ENRICH_BATCH` distinct node ids, kept sorted so the same set always
/// has the same key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrichBatch {
    nodes: Vec<NodeId>,
}

impl EnrichBatch {
    pub fn new(mut nodes: Vec<NodeId>) -> Result<Self, WorkError> {
        nodes.sort();
        nodes.dedup();
        if nodes.is_empty() || nodes.len() > ENRICH_BATCH {
            return Err(WorkError::BatchSize(nodes.len()));
        }
        Ok(Self { nodes })
    }

    pub fn ids(&self) -> Vec<&str> {
        self.nodes.iter().map(NodeId::as_str).collect()
    }

    fn payload(&self) -> String {
        let ids: Vec<&str> = self.nodes.iter().map(NodeId::as_str).collect();
        ids.join(",")
    }

    fn from_payload(payload: &str) -> Result<Self, WorkError> {
        let nodes = payload
            .split(',')
            .map(NodeId::parse)
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(nodes)
    }

    fn key(&self) -> String {
        short_hash(&self.payload())
    }
}

fn short_hash(payload: &str) -> String {
    let digest = Sha256::digest(payload.as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

fn cursor_part(after: Option<&str>) -> &str {
    after.unwrap_or(START)
}

fn parse_cursor(raw: &str) -> Option<Option<String>> {
    match raw {
        START => Some(None),
        cursor if valid_cursor(cursor) => Some(Some(cursor.to_owned())),
        _ => None,
    }
}

/// A calendar day, held as 00:00 UTC and written `YYYY-MM-DD`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Day(Timestamp);

impl Day {
    pub fn of(at: Timestamp) -> Self {
        Self(Timestamp(
            at.0.div_euclid(SECONDS_PER_DAY) * SECONDS_PER_DAY,
        ))
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let at = Timestamp::parse_rfc3339(&format!("{raw}T00:00:00Z")).ok()?;
        let day = Self(at);
        (day.date() == raw).then_some(day)
    }

    pub fn year_start(year: i32) -> Option<Self> {
        Self::parse(&format!("{year:04}-01-01"))
    }

    pub fn year_end(year: i32) -> Option<Self> {
        Self::parse(&format!("{year:04}-12-31"))
    }

    pub fn date(self) -> String {
        self.0.to_string().chars().take(10).collect()
    }

    pub fn year(self) -> i32 {
        self.date()
            .get(..4)
            .and_then(|y| y.parse::<u32>().ok())
            .and_then(|y| i32::try_from(y).ok())
            .unwrap_or_default()
    }

    pub fn start(self) -> Timestamp {
        self.0
    }

    pub fn plus_days(self, days: i64) -> Self {
        Self(Timestamp(self.0 .0 + days * SECONDS_PER_DAY))
    }

    /// Days from `self` to `last`, both included.
    pub fn days_through(self, last: Day) -> i64 {
        (last.0 .0 - self.0 .0).div_euclid(SECONDS_PER_DAY) + 1
    }
}

/// The pull requests of one `Reviews` unit: 1 to `IMPORT_PAGE` distinct node ids,
/// sorted so the same set always has the same key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrSet(Vec<NodeId>);

impl PrSet {
    pub fn new(mut prs: Vec<NodeId>) -> Option<Self> {
        prs.sort();
        prs.dedup();
        (!prs.is_empty() && prs.len() <= IMPORT_PAGE).then_some(Self(prs))
    }

    pub fn ids(&self) -> Vec<&str> {
        self.0.iter().map(NodeId::as_str).collect()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    fn payload(&self) -> String {
        self.ids().join(",")
    }

    fn from_payload(payload: &str) -> Option<Self> {
        let prs = payload
            .split(',')
            .map(NodeId::parse)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        Self::new(prs)
    }
}

/// One unit of the history import (ADR 0010). Each is a `backfill_page` row under
/// an `import.` key, so it shares that kind's rate gate, freshness, and dead-row
/// revival. `Start` reads my account's creation date and queues the years.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportKey {
    Start,
    SearchRange {
        from: Day,
        to: Day,
        after: Option<String>,
    },
    Timeline {
        pr: NodeId,
        after: String,
    },
    ReviewedPrs {
        year: i32,
        after: Option<String>,
    },
    /// A cursor pages the reviews of one pull request, so it comes with one id.
    Reviews {
        prs: PrSet,
        after: Option<String>,
    },
}

impl ImportKey {
    pub fn is_import_key(key: &str) -> bool {
        key.split('.').next() == Some(IMPORT_PREFIX)
    }

    pub fn key(&self) -> String {
        match self {
            Self::Start => format!("{IMPORT_PREFIX}.{START}"),
            Self::SearchRange { from, to, after } => format!(
                "{IMPORT_PREFIX}.range.{}.{}.{}",
                from.date(),
                to.date(),
                cursor_part(after.as_deref())
            ),
            Self::Timeline { pr, after } => {
                format!("{IMPORT_PREFIX}.timeline.{}.{after}", pr.as_str())
            }
            Self::ReviewedPrs { year, after } => format!(
                "{IMPORT_PREFIX}.reviewed.{year:04}.{}",
                cursor_part(after.as_deref())
            ),
            Self::Reviews { prs, after } => format!(
                "{IMPORT_PREFIX}.reviews.{}.{}",
                short_hash(&prs.payload()),
                cursor_part(after.as_deref())
            ),
        }
    }

    fn payload(&self) -> Option<String> {
        match self {
            Self::Reviews { prs, .. } => Some(prs.payload()),
            _ => None,
        }
    }

    pub fn parse(key: &str, payload: Option<&str>) -> Option<Self> {
        let parts: Vec<&str> = key.split('.').collect();
        let parsed = match parts.as_slice() {
            [IMPORT_PREFIX, START] => Self::Start,
            [IMPORT_PREFIX, "range", from, to, after] => {
                let (from, to) = (Day::parse(from)?, Day::parse(to)?);
                (from <= to).then_some(())?;
                Self::SearchRange {
                    from,
                    to,
                    after: parse_cursor(after)?,
                }
            }
            [IMPORT_PREFIX, "timeline", pr, after] => Self::Timeline {
                pr: NodeId::parse(pr).ok()?,
                after: parse_cursor(after)??,
            },
            [IMPORT_PREFIX, "reviewed", year, after] if year.len() == 4 => Self::ReviewedPrs {
                year: i32::try_from(year.parse::<u32>().ok()?).ok()?,
                after: parse_cursor(after)?,
            },
            [IMPORT_PREFIX, "reviews", _, after] => {
                let prs = PrSet::from_payload(payload?)?;
                let after = parse_cursor(after)?;
                (after.is_none() || prs.len() == 1).then_some(())?;
                Self::Reviews { prs, after }
            }
            _ => return None,
        };
        (parsed.key() == key).then_some(parsed)
    }
}

/// One page of the weekly repo refresh: up to `IMPORT_PAGE` repos after `after`,
/// in name order. A `backfill_page` row under a `repos.` key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoPage {
    pub week: WeekStart,
    pub after: Option<RepoName>,
}

impl RepoPage {
    pub fn first(week: WeekStart) -> Self {
        Self { week, after: None }
    }

    pub fn is_repos_key(key: &str) -> bool {
        key.split('.').next() == Some(REPOS_PREFIX)
    }

    pub fn key(&self) -> String {
        let after = self
            .after
            .as_ref()
            .map_or_else(|| START.to_owned(), RepoName::to_string);
        format!("{REPOS_PREFIX}.{}.{after}", self.week.date())
    }

    pub fn parse(key: &str) -> Option<Self> {
        let mut parts = key.splitn(3, '.');
        (parts.next()? == REPOS_PREFIX).then_some(())?;
        let week = WeekStart::parse_date(parts.next()?)?;
        let after = match parts.next()? {
            START => None,
            name => Some(RepoName::parse(name).ok()?),
        };
        let parsed = Self { week, after };
        (parsed.key() == key).then_some(parsed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkKind {
    PollNotifications,
    Enrich(EnrichBatch),
    BackfillPage(BackfillCursor),
    /// Stored as a `backfill_page` row under a `seed.` key, so it shares that kind's
    /// rate gate, freshness, and dead-row revival.
    SeedHistory(HistoryKey),
    /// Stored as a `backfill_page` row under an `import.` key.
    ImportFacts(ImportKey),
    /// Stored as a `backfill_page` row under a `repos.` key.
    RefreshRepos(RepoPage),
    Rollup(RollupKey),
    SendAlerts,
}

/// The `kind`, `key`, and `payload` columns of a `work` row. `(kind, key)` is the
/// primary key, so enqueueing the same unit twice is one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Columns {
    pub kind: SyncKind,
    pub key: String,
    pub payload: Option<String>,
}

impl WorkKind {
    pub fn kind(&self) -> SyncKind {
        match self {
            Self::PollNotifications => SyncKind::PollNotifications,
            Self::Enrich(_) => SyncKind::Enrich,
            Self::BackfillPage(_)
            | Self::SeedHistory(_)
            | Self::ImportFacts(_)
            | Self::RefreshRepos(_) => SyncKind::BackfillPage,
            Self::Rollup(_) => SyncKind::Rollup,
            Self::SendAlerts => SyncKind::SendAlerts,
        }
    }

    pub fn key(&self) -> String {
        match self {
            Self::PollNotifications => MAIN_KEY.to_owned(),
            Self::Enrich(batch) => batch.key(),
            Self::BackfillPage(cursor) => cursor.key(),
            Self::SeedHistory(key) => key.key(),
            Self::ImportFacts(key) => key.key(),
            Self::RefreshRepos(page) => page.key(),
            Self::Rollup(key) => key.key(),
            Self::SendAlerts => MAIN_KEY.to_owned(),
        }
    }

    pub fn columns(&self) -> Columns {
        Columns {
            kind: self.kind(),
            key: self.key(),
            payload: match self {
                Self::Enrich(batch) => Some(batch.payload()),
                Self::ImportFacts(key) => key.payload(),
                _ => None,
            },
        }
    }

    pub fn is_import(&self) -> bool {
        matches!(self, Self::ImportFacts(_))
    }

    /// Import and repo refresh units wait for `SyncState::paced_until`.
    pub fn is_paced(&self) -> bool {
        matches!(self, Self::ImportFacts(_) | Self::RefreshRepos(_))
    }

    pub fn from_columns(kind: &str, key: &str, payload: Option<&str>) -> Result<Self, WorkError> {
        let kind = SyncKind::parse(kind).ok_or_else(|| WorkError::BadKind(kind.to_owned()))?;
        let bad_key = || WorkError::BadKey {
            kind,
            key: key.to_owned(),
        };
        let work = match kind {
            SyncKind::PollNotifications if key == MAIN_KEY => Self::PollNotifications,
            SyncKind::SendAlerts if key == MAIN_KEY => Self::SendAlerts,
            SyncKind::Rollup => Self::Rollup(RollupKey::parse(key).ok_or_else(bad_key)?),
            SyncKind::BackfillPage if HistoryKey::is_seed_key(key) => {
                Self::SeedHistory(HistoryKey::parse(key).ok_or_else(bad_key)?)
            }
            SyncKind::BackfillPage if ImportKey::is_import_key(key) => {
                Self::ImportFacts(ImportKey::parse(key, payload).ok_or_else(bad_key)?)
            }
            SyncKind::BackfillPage if RepoPage::is_repos_key(key) => {
                Self::RefreshRepos(RepoPage::parse(key).ok_or_else(bad_key)?)
            }
            SyncKind::BackfillPage => {
                Self::BackfillPage(BackfillCursor::parse(key).ok_or_else(bad_key)?)
            }
            SyncKind::Enrich => {
                let batch = EnrichBatch::from_payload(payload.ok_or(WorkError::MissingPayload)?)?;
                if batch.key() != key {
                    return Err(bad_key());
                }
                Self::Enrich(batch)
            }
            SyncKind::PollNotifications | SyncKind::SendAlerts => return Err(bad_key()),
        };
        Ok(work)
    }

    #[cfg(any(test, feature = "probe"))]
    /// Reads a unit named the way the probe route names it: an `enrich` key is the
    /// comma-separated node ids, and every other key is the column key.
    pub fn from_request(kind: &str, key: &str) -> Result<Self, WorkError> {
        if kind == SyncKind::Enrich.as_str() {
            return EnrichBatch::from_payload(key).map(Self::Enrich);
        }
        Self::from_columns(kind, key, None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    Pending,
    Dead,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkRow {
    pub kind: WorkKind,
    pub due_at: Timestamp,
    pub attempts: u32,
    pub state: RowState,
    pub last_error: Option<String>,
}

impl WorkRow {
    pub fn pending(kind: WorkKind, due_at: Timestamp) -> Self {
        Self {
            kind,
            due_at,
            attempts: 0,
            state: RowState::Pending,
            last_error: None,
        }
    }

    pub fn is_dead(&self) -> bool {
        self.state == RowState::Dead
    }

    pub fn revived(self, due_at: Timestamp) -> Self {
        Self::pending(self.kind, due_at)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KickReason {
    Reconcile,
    Email,
    Webhook,
    SignedIn,
    Manual,
    Rollup,
}

impl KickReason {
    const ALL: [KickReason; 6] = [
        KickReason::Reconcile,
        KickReason::Email,
        KickReason::Webhook,
        KickReason::SignedIn,
        KickReason::Manual,
        KickReason::Rollup,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reconcile => "reconcile",
            Self::Email => "email",
            Self::Webhook => "webhook",
            Self::SignedIn => "signed_in",
            Self::Manual => "manual",
            Self::Rollup => "rollup",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.as_str() == s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::backfill::{SearchKind, SearchWindow};

    fn ids(raw: &[&str]) -> Vec<NodeId> {
        raw.iter().map(|id| NodeId::parse(id).unwrap()).collect()
    }

    fn enrich(raw: &[&str]) -> WorkKind {
        WorkKind::Enrich(EnrichBatch::new(ids(raw)).unwrap())
    }

    #[test]
    fn same_unit_twice_is_one_key() {
        assert_eq!(
            enrich(&["PR_a", "I_b"]).key(),
            enrich(&["PR_a", "I_b"]).key()
        );
        assert_eq!(WorkKind::PollNotifications.key(), "main");
        assert_eq!(WorkKind::Rollup(RollupKey::Nightly).key(), "nightly");
        assert_eq!(
            WorkKind::BackfillPage(BackfillCursor::first(
                SearchWindow::Backfill90d,
                Timestamp(0)
            ))
            .key(),
            "b90.1969-10-03.authored.1.start"
        );
        assert_ne!(enrich(&["PR_a"]).key(), enrich(&["PR_b"]).key());
    }

    #[test]
    fn send_alerts_enqueued_twice_is_one_row() {
        use crate::sync::scheduler::enqueue;
        let first = enqueue(&[], WorkKind::SendAlerts, Timestamp(5)).unwrap();
        assert_eq!(first.kind.key(), "main");
        assert_eq!(enqueue(&[first], WorkKind::SendAlerts, Timestamp(9)), None);
        assert!(WorkKind::from_columns("send_alerts", "other", None).is_err());
    }

    #[test]
    fn batch_key_ignores_id_order() {
        assert_eq!(
            enrich(&["PR_c", "PR_a", "I_b"]).key(),
            enrich(&["I_b", "PR_c", "PR_a"]).key()
        );
        assert_eq!(enrich(&["PR_a", "PR_a"]).key(), enrich(&["PR_a"]).key());
    }

    #[test]
    fn kind_round_trips_through_columns() {
        for kind in [
            WorkKind::PollNotifications,
            enrich(&["PR_z", "I_y"]),
            WorkKind::BackfillPage(BackfillCursor::first(
                SearchWindow::Backfill90d,
                Timestamp(0),
            )),
            WorkKind::BackfillPage(BackfillCursor {
                window: SearchWindow::Nightly7d,
                since: "2026-10-31".into(),
                kind: SearchKind::Involves,
                page: 3,
                after: Some("Y3Vyc29yOjI=".into()),
            }),
            WorkKind::Rollup(RollupKey::Nightly),
            WorkKind::SeedHistory(HistoryKey::first(Timestamp(1_794_052_800))),
            WorkKind::SendAlerts,
        ] {
            let c = kind.columns();
            let back = WorkKind::from_columns(c.kind.as_str(), &c.key, c.payload.as_deref());
            assert_eq!(back, Ok(kind));
        }
    }

    fn day(raw: &str) -> Day {
        Day::parse(raw).unwrap()
    }

    fn prs(raw: &[&str]) -> PrSet {
        PrSet::new(ids(raw)).unwrap()
    }

    fn import_and_repo_units() -> Vec<WorkKind> {
        let week = WeekStart::parse_date("2026-11-09").unwrap();
        [
            ImportKey::Start,
            ImportKey::SearchRange {
                from: day("2025-01-01"),
                to: day("2025-12-31"),
                after: None,
            },
            ImportKey::SearchRange {
                from: day("2025-04-02"),
                to: day("2025-07-02"),
                after: Some("Y3Vyc29yOjUw".into()),
            },
            ImportKey::Timeline {
                pr: NodeId::parse("PR_kwDOAbc-1_=").unwrap(),
                after: "Y3I6MjA=".into(),
            },
            ImportKey::ReviewedPrs {
                year: 2024,
                after: None,
            },
            ImportKey::ReviewedPrs {
                year: 2026,
                after: Some("Y3I=".into()),
            },
            ImportKey::Reviews {
                prs: prs(&["PR_b", "PR_a", "PR_c"]),
                after: None,
            },
            ImportKey::Reviews {
                prs: prs(&["PR_a"]),
                after: Some("cmV2OjEwMA==".into()),
            },
        ]
        .into_iter()
        .map(WorkKind::ImportFacts)
        .chain([
            WorkKind::RefreshRepos(RepoPage::first(week)),
            WorkKind::RefreshRepos(RepoPage {
                week,
                after: Some(RepoName::parse("octo/widgets.js").unwrap()),
            }),
        ])
        .collect()
    }

    #[test]
    fn import_and_repos_keys_round_trip_through_columns_as_backfill_pages() {
        for kind in import_and_repo_units() {
            let c = kind.columns();
            assert_eq!(c.kind, SyncKind::BackfillPage, "{}", c.key);
            assert!(
                c.key.starts_with("import.") || c.key.starts_with("repos."),
                "{}",
                c.key
            );
            let back = WorkKind::from_columns(c.kind.as_str(), &c.key, c.payload.as_deref());
            assert_eq!(back, Ok(kind));
        }
        assert_eq!(
            WorkKind::ImportFacts(ImportKey::Start).key(),
            "import.start"
        );
        assert_eq!(
            import_and_repo_units()[9].key(),
            "repos.2026-11-09.octo/widgets.js"
        );
    }

    #[test]
    fn import_and_repos_keys_reject_everything_else() {
        let payload = Some("PR_a,PR_b");
        for bad in [
            "import",
            "import.",
            "import.stop",
            "import.range.2025-01-01.2025-12-31",
            "import.range.2025-12-31.2025-01-01.start",
            "import.range.2025-02-30.2025-12-31.start",
            "import.range.2025-01-01.2025-12-31.a.b",
            "import.timeline.PR_a.start",
            "import.timeline.PR,a.Y3I=",
            "import.reviewed.25.start",
            "import.reviewed.2025.",
            "import.reviews.0000000000000000.start",
            "repos",
            "repos.2026-11-10.start",
            "repos.2026-11-09.",
            "repos.2026-11-09.widgets",
        ] {
            assert!(
                WorkKind::from_columns("backfill_page", bad, payload).is_err(),
                "{bad}"
            );
        }
        let two = ImportKey::Reviews {
            prs: prs(&["PR_a", "PR_b"]),
            after: None,
        };
        let paged = two.key().replace(".start", ".Y3I=");
        assert!(WorkKind::from_columns("backfill_page", &paged, payload).is_err());
        assert!(WorkKind::from_columns("backfill_page", &two.key(), None).is_err());
        let many: Vec<String> = (0..=IMPORT_PAGE).map(|n| format!("PR_{n}")).collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        assert!(PrSet::new(ids(&many)).is_none());
        assert!(PrSet::new(ids(&many[..IMPORT_PAGE])).is_some());
    }

    #[test]
    fn a_day_reads_its_year_and_counts_days() {
        let first = day("2024-01-01");
        assert_eq!(first.year(), 2024);
        assert_eq!(first.days_through(day("2024-12-31")), 366);
        assert_eq!(first.plus_days(59).date(), "2024-02-29");
        assert_eq!(Day::of(Timestamp(first.start().0 + 86_399)), first);
        assert_eq!(Day::year_end(2025), Some(day("2025-12-31")));
    }

    #[test]
    fn columns_that_do_not_fit_the_kind_are_rejected() {
        assert!(WorkKind::from_columns("poll_notifications", "other", None).is_err());
        assert!(WorkKind::from_columns("rollup", "main", None).is_err());
        assert!(WorkKind::from_columns("seed_history", "x", None).is_err());
        assert!(WorkKind::from_columns("backfill_page", "", None).is_err());
        assert_eq!(
            WorkKind::from_columns("enrich", "abc", None),
            Err(WorkError::MissingPayload)
        );
        let c = enrich(&["PR_a"]).columns();
        assert!(
            WorkKind::from_columns("enrich", "0000000000000000", c.payload.as_deref()).is_err()
        );
    }

    #[test]
    fn a_batch_holds_one_to_enrich_batch_ids() {
        assert_eq!(EnrichBatch::new(vec![]), Err(WorkError::BatchSize(0)));
        let many: Vec<String> = (0..=ENRICH_BATCH).map(|n| format!("PR_{n}")).collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        assert_eq!(
            EnrichBatch::new(ids(&many)),
            Err(WorkError::BatchSize(ENRICH_BATCH + 1))
        );
        assert!(EnrichBatch::new(ids(&many[..ENRICH_BATCH])).is_ok());
    }

    #[test]
    fn node_ids_reject_separators_and_blanks() {
        for bad in ["", "a,b", "a b", "a/b", &"x".repeat(MAX_NODE_ID_LEN + 1)] {
            assert!(NodeId::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_request_names_an_enrich_unit_by_its_node_ids() {
        assert_eq!(
            WorkKind::from_request("enrich", "PR_b,PR_a"),
            Ok(enrich(&["PR_a", "PR_b"]))
        );
        assert_eq!(
            WorkKind::from_request("rollup", "nightly"),
            Ok(WorkKind::Rollup(RollupKey::Nightly))
        );
        assert!(WorkKind::from_request("enrich", "").is_err());
    }

    #[test]
    fn kick_reasons_round_trip() {
        for reason in KickReason::ALL {
            assert_eq!(KickReason::parse(reason.as_str()), Some(reason));
            let json = serde_json::to_string(&reason).unwrap();
            assert_eq!(json, format!("\"{}\"", reason.as_str()));
        }
        assert_eq!(KickReason::parse("nightly"), None);
    }
}
