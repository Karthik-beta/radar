//! One unit of Syncer work and how it maps to a row of the `work` table.

use std::fmt;

use domain::ids::Timestamp;
use domain::sync_status::SyncKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::config::ENRICH_BATCH;

const MAIN_KEY: &str = "main";
const NIGHTLY_KEY: &str = "nightly";
const START_KEY: &str = "start";
const MAX_NODE_ID_LEN: usize = 128;

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
        let digest = Sha256::digest(self.payload().as_bytes());
        digest[..8].iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Where a backfill page starts: the first page, or after a GraphQL cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cursor {
    Start,
    After(String),
}

impl Cursor {
    pub fn parse(raw: &str) -> Result<Self, WorkError> {
        match raw {
            START_KEY => Ok(Self::Start),
            "" => Err(WorkError::BadKey {
                kind: SyncKind::BackfillPage,
                key: raw.to_owned(),
            }),
            cursor => Ok(Self::After(cursor.to_owned())),
        }
    }

    fn key(&self) -> &str {
        match self {
            Self::Start => START_KEY,
            Self::After(cursor) => cursor,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkKind {
    PollNotifications,
    Enrich(EnrichBatch),
    BackfillPage(Cursor),
    Rollup,
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
            Self::BackfillPage(_) => SyncKind::BackfillPage,
            Self::Rollup => SyncKind::Rollup,
        }
    }

    pub fn key(&self) -> String {
        match self {
            Self::PollNotifications => MAIN_KEY.to_owned(),
            Self::Enrich(batch) => batch.key(),
            Self::BackfillPage(cursor) => cursor.key().to_owned(),
            Self::Rollup => NIGHTLY_KEY.to_owned(),
        }
    }

    pub fn columns(&self) -> Columns {
        Columns {
            kind: self.kind(),
            key: self.key(),
            payload: match self {
                Self::Enrich(batch) => Some(batch.payload()),
                _ => None,
            },
        }
    }

    pub fn from_columns(kind: &str, key: &str, payload: Option<&str>) -> Result<Self, WorkError> {
        let kind = SyncKind::parse(kind).ok_or_else(|| WorkError::BadKind(kind.to_owned()))?;
        let bad_key = || WorkError::BadKey {
            kind,
            key: key.to_owned(),
        };
        let work = match kind {
            SyncKind::PollNotifications if key == MAIN_KEY => Self::PollNotifications,
            SyncKind::Rollup if key == NIGHTLY_KEY => Self::Rollup,
            SyncKind::BackfillPage => Self::BackfillPage(Cursor::parse(key)?),
            SyncKind::Enrich => {
                let batch = EnrichBatch::from_payload(payload.ok_or(WorkError::MissingPayload)?)?;
                if batch.key() != key {
                    return Err(bad_key());
                }
                Self::Enrich(batch)
            }
            SyncKind::PollNotifications | SyncKind::Rollup => return Err(bad_key()),
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
    #[cfg(any(test, feature = "probe"))]
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

    #[cfg(any(test, feature = "probe"))]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|reason| reason.as_str() == s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(WorkKind::Rollup.key(), "nightly");
        assert_eq!(WorkKind::BackfillPage(Cursor::Start).key(), "start");
        assert_ne!(enrich(&["PR_a"]).key(), enrich(&["PR_b"]).key());
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
            WorkKind::BackfillPage(Cursor::Start),
            WorkKind::BackfillPage(Cursor::After("Y3Vyc29yOjI=".into())),
            WorkKind::Rollup,
        ] {
            let c = kind.columns();
            let back = WorkKind::from_columns(c.kind.as_str(), &c.key, c.payload.as_deref());
            assert_eq!(back, Ok(kind));
        }
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
            Ok(WorkKind::Rollup)
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
