use domain::activity::{Actor, ActorKind};
use domain::ids::{ItemRef, Login, RepoName, Timestamp};
use domain::item::{CiState, Item, ItemState, ReviewDecision};
use domain::stage::Stage;
use domain::turn::{Classification, Turn};
use serde::Deserialize;

use super::rows::{count, flag, untag, untag_opt, RowError, Tagged, TurnColumns};
use super::sql::{Param, Statement};

/// The `items` columns in table order. The upsert and the dashboard queries
/// both build their SQL from this list.
macro_rules! item_columns {
    () => {
        "node_id, repo, number, kind, title, url, head_ref, author_login, author_kind, \
         authored_by_me, state, created_at, updated_at, closed_at, turn, reason, since, stage, \
         ci_state, review_decision, last_human_at, last_human_login, last_human_kind, \
         last_human_url, synced_at"
    };
}

/// The update leaves out `repo`, `number`, `kind`, the author columns, and
/// `created_at`. SQLite rewrites every index that mentions an assigned column,
/// even to the same value, so assigning them would push one upsert past the
/// seven-row write budget. They do not change after an item is created.
const UPSERT: &str = concat!(
    "INSERT INTO items (",
    item_columns!(),
    ") VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, \
     ?19, ?20, ?21, ?22, ?23, ?24, ?25) \
     ON CONFLICT (node_id) DO UPDATE SET \
     title = excluded.title, url = excluded.url, head_ref = excluded.head_ref, \
     state = excluded.state, updated_at = excluded.updated_at, \
     closed_at = excluded.closed_at, turn = excluded.turn, reason = excluded.reason, \
     since = excluded.since, stage = excluded.stage, ci_state = excluded.ci_state, \
     review_decision = excluded.review_decision, last_human_at = excluded.last_human_at, \
     last_human_login = excluded.last_human_login, last_human_kind = excluded.last_human_kind, \
     last_human_url = excluded.last_human_url, synced_at = excluded.synced_at \
     WHERE excluded.updated_at >= items.updated_at"
);

/// What the classifier does not know: GitHub's node id and the display fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemMeta {
    pub node_id: String,
    pub title: String,
    pub url: String,
    pub head_ref: Option<String>,
    pub last_human_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastHuman {
    pub at: Timestamp,
    pub login: Login,
    pub kind: String,
    pub url: Option<String>,
}

/// One row of `items`, typed. `turn` and `reason` live in one `Turn`, so the
/// pair `turn = 'me'` without a reason cannot be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRow {
    pub node_id: String,
    pub item_ref: ItemRef,
    pub title: String,
    pub url: String,
    pub head_ref: Option<String>,
    pub author: Actor,
    pub authored_by_me: bool,
    pub state: ItemState,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub closed_at: Option<Timestamp>,
    pub turn: Turn,
    pub since: Timestamp,
    pub stage: Option<Stage>,
    pub ci_state: Option<CiState>,
    pub review_decision: Option<ReviewDecision>,
    pub last_human: Option<LastHuman>,
    pub synced_at: Timestamp,
}

impl ItemRow {
    pub fn from_classified(
        meta: ItemMeta,
        item: &Item,
        me: &Login,
        classification: &Classification,
        synced_at: Timestamp,
    ) -> Self {
        let closed = matches!(item.state, ItemState::Merged | ItemState::Closed);
        let last_human = classification.last_human.as_ref().map(|a| LastHuman {
            at: a.at,
            login: a.actor.login.clone(),
            kind: a.kind.tag().to_owned(),
            url: meta.last_human_url,
        });
        Self {
            node_id: meta.node_id,
            item_ref: item.item_ref.clone(),
            title: meta.title,
            url: meta.url,
            head_ref: meta.head_ref,
            author: item.author.clone(),
            authored_by_me: item.author.login == *me,
            state: item.state,
            created_at: item.created_at,
            updated_at: item.updated_at,
            closed_at: item
                .closed_at
                .or_else(|| closed.then_some(classification.since)),
            turn: classification.turn,
            since: classification.since,
            stage: classification.stage,
            ci_state: item.ci.map(|ci| ci.state),
            review_decision: item.review_decision,
            last_human,
            synced_at,
        }
    }
}

pub fn upsert(row: &ItemRow) -> Statement {
    let turn = TurnColumns::from(row.turn);
    let human = row.last_human.as_ref();
    Statement {
        sql: UPSERT,
        params: vec![
            Param::text(&row.node_id),
            Param::text(row.item_ref.repo.to_string()),
            Param::Int(i64::from(row.item_ref.number)),
            Param::text(row.item_ref.kind.tag()),
            Param::text(&row.title),
            Param::text(&row.url),
            Param::opt_text(row.head_ref.as_deref()),
            Param::text(row.author.login.as_str()),
            Param::text(row.author.kind.tag()),
            Param::flag(row.authored_by_me),
            Param::text(row.state.tag()),
            Param::ts(row.created_at),
            Param::ts(row.updated_at),
            Param::opt_ts(row.closed_at),
            Param::text(turn.turn),
            Param::opt_text(turn.reason),
            Param::ts(row.since),
            Param::opt_text(row.stage.map(Stage::tag)),
            Param::opt_text(row.ci_state.map(CiState::tag)),
            Param::opt_text(row.review_decision.map(ReviewDecision::tag)),
            Param::opt_ts(human.map(|h| h.at)),
            Param::opt_text(human.map(|h| h.login.as_str())),
            Param::opt_text(human.map(|h| h.kind.as_str())),
            Param::opt_text(human.and_then(|h| h.url.as_deref())),
            Param::ts(row.synced_at),
        ],
    }
}

/// An `items` row as D1 returns it, before the columns are checked.
#[derive(Debug, Clone, Deserialize)]
pub struct ItemRecord {
    pub node_id: String,
    pub repo: String,
    pub number: i64,
    pub kind: String,
    pub title: String,
    pub url: String,
    pub head_ref: Option<String>,
    pub author_login: String,
    pub author_kind: String,
    pub authored_by_me: i64,
    pub state: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub closed_at: Option<i64>,
    pub turn: String,
    pub reason: Option<String>,
    pub since: i64,
    pub stage: Option<String>,
    pub ci_state: Option<String>,
    pub review_decision: Option<String>,
    pub last_human_at: Option<i64>,
    pub last_human_login: Option<String>,
    pub last_human_kind: Option<String>,
    pub last_human_url: Option<String>,
    pub synced_at: i64,
}

fn login(column: &'static str, value: &str) -> Result<Login, RowError> {
    Login::parse(value).map_err(|source| RowError::BadId { column, source })
}

impl TryFrom<ItemRecord> for ItemRow {
    type Error = RowError;

    fn try_from(r: ItemRecord) -> Result<Self, RowError> {
        let repo = RepoName::parse(&r.repo).map_err(|source| RowError::BadId {
            column: "repo",
            source,
        })?;
        let last_human = match (r.last_human_at, r.last_human_login, r.last_human_kind) {
            (Some(at), Some(who), Some(kind)) => Some(LastHuman {
                at: Timestamp(at),
                login: login("last_human_login", &who)?,
                kind,
                url: r.last_human_url,
            }),
            (None, None, None) => None,
            _ => return Err(RowError::PartialLastHuman),
        };
        Ok(Self {
            node_id: r.node_id,
            item_ref: ItemRef {
                repo,
                number: count("number", r.number)?,
                kind: untag("kind", &r.kind)?,
            },
            title: r.title,
            url: r.url,
            head_ref: r.head_ref,
            author: Actor {
                login: login("author_login", &r.author_login)?,
                kind: untag::<ActorKind>("author_kind", &r.author_kind)?,
            },
            authored_by_me: flag("authored_by_me", r.authored_by_me)?,
            state: untag("state", &r.state)?,
            created_at: Timestamp(r.created_at),
            updated_at: Timestamp(r.updated_at),
            closed_at: r.closed_at.map(Timestamp),
            turn: Turn::try_from(TurnColumns {
                turn: r.turn,
                reason: r.reason,
            })?,
            since: Timestamp(r.since),
            stage: untag_opt("stage", r.stage.as_deref())?,
            ci_state: untag_opt("ci_state", r.ci_state.as_deref())?,
            review_decision: untag_opt("review_decision", r.review_decision.as_deref())?,
            last_human,
            synced_at: Timestamp(r.synced_at),
        })
    }
}

#[cfg(any(test, feature = "sql-dump"))]
impl ItemMeta {
    /// Deterministic display fields for an item that has no GitHub node.
    pub fn placeholder(item_ref: &ItemRef) -> Self {
        let path = match item_ref.kind {
            domain::ids::ItemKind::Pr => "pull",
            domain::ids::ItemKind::Issue => "issues",
        };
        Self {
            node_id: item_ref.to_string(),
            title: format!("Placeholder title for {item_ref}"),
            url: format!(
                "https://github.com/{}/{path}/{}",
                item_ref.repo, item_ref.number
            ),
            head_ref: None,
            last_human_url: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::classify::classify;
    use serde::de::IgnoredAny;

    use super::*;
    use crate::db::testing::{count_rows, exec, item_row, memory_db, opcodes, query_json};

    const HOSTILE_TITLES: [&str; 5] = [
        "it's \"quoted\"",
        "emoji 🚀 and 日本語 and \u{202e}bidi",
        "'); DROP TABLE items;--",
        "line one\nline two\r\nand a nul \0 inside",
        "",
    ];

    fn param_index(name: &str) -> usize {
        item_columns!()
            .split(", ")
            .position(|c| c == name)
            .unwrap_or(usize::MAX)
    }

    fn with_param(mut stmt: Statement, column: &str, value: Param) -> Statement {
        stmt.params[param_index(column)] = value;
        stmt
    }

    fn record(conn: &rusqlite::Connection, node_id: &str) -> ItemRecord {
        let rows = query_json(
            conn,
            "SELECT * FROM items WHERE node_id = ?1",
            &[Param::text(node_id)],
        );
        assert_eq!(rows.len(), 1);
        serde_json::from_value(serde_json::Value::Object(rows[0].clone())).expect("record")
    }

    #[test]
    fn upsert_twice_keeps_one_row() {
        let conn = memory_db();
        let mut row = item_row(1);
        exec(&conn, &upsert(&row)).expect("first upsert");
        row.synced_at = Timestamp(9000);
        exec(&conn, &upsert(&row)).expect("second upsert");
        assert_eq!(count_rows(&conn, "items"), 1);
        assert_eq!(record(&conn, &row.node_id).synced_at, 9000);
    }

    #[test]
    fn older_update_does_not_overwrite_newer() {
        let conn = memory_db();
        let mut newer = item_row(1);
        newer.title = "newer".to_owned();
        newer.updated_at = Timestamp(3000);
        let mut older = item_row(1);
        older.title = "older".to_owned();
        older.updated_at = Timestamp(2000);
        exec(&conn, &upsert(&newer)).expect("newer");
        exec(&conn, &upsert(&older)).expect("older");
        let stored = record(&conn, &newer.node_id);
        assert_eq!((stored.title.as_str(), stored.updated_at), ("newer", 3000));
        assert_eq!(count_rows(&conn, "items"), 1);
    }

    #[test]
    fn update_rewrites_only_the_partial_indexes() {
        let conn = memory_db();
        let ops = opcodes(&conn, &upsert(&item_row(1)));
        let mut rewritten: Vec<&str> = ops
            .iter()
            .filter(|(op, _)| op == "IdxDelete")
            .map(|(_, index)| index.as_str())
            .collect();
        rewritten.sort_unstable();
        assert_eq!(
            rewritten,
            [
                "idx_items_closed",
                "idx_items_me_since",
                "idx_items_pipeline",
                "idx_items_them_since"
            ]
        );
    }

    #[test]
    fn turn_me_without_reason_is_rejected() {
        let conn = memory_db();
        let mut row = item_row(1);
        row.turn = Turn::Me(domain::turn::Reason::CiFailed);
        let stmt = with_param(upsert(&row), "reason", Param::Null);
        let err = exec(&conn, &stmt).expect_err("must fail");
        assert!(err.to_string().contains("CHECK"), "{err}");
        assert_eq!(count_rows(&conn, "items"), 0);
    }

    #[test]
    fn turn_them_with_reason_is_rejected() {
        let conn = memory_db();
        let stmt = with_param(upsert(&item_row(1)), "reason", Param::text("ci_failed"));
        let err = exec(&conn, &stmt).expect_err("must fail");
        assert!(err.to_string().contains("CHECK"), "{err}");
        assert_eq!(count_rows(&conn, "items"), 0);
    }

    #[test]
    fn unknown_stage_is_rejected() {
        let conn = memory_db();
        let stmt = with_param(upsert(&item_row(1)), "stage", Param::text("bogus"));
        let err = exec(&conn, &stmt).expect_err("must fail");
        assert!(err.to_string().contains("CHECK"), "{err}");
        for stage in Stage::VARIANTS {
            let row = ItemRow {
                stage: Some(*stage),
                ..item_row(2)
            };
            exec(&conn, &upsert(&row)).expect("every domain stage is accepted");
        }
    }

    #[test]
    fn every_domain_enum_value_is_accepted_by_the_checks() {
        let conn = memory_db();
        let mut n = 0;
        let mut put = |row: ItemRow| {
            n += 1;
            let row = ItemRow {
                node_id: format!("N{n}"),
                item_ref: ItemRef {
                    number: n,
                    ..row.item_ref.clone()
                },
                ..row
            };
            exec(&conn, &upsert(&row)).expect("accepted");
        };
        for state in ItemState::VARIANTS {
            put(ItemRow {
                state: *state,
                closed_at: Some(Timestamp(1)),
                ..item_row(1)
            });
        }
        for kind in domain::ids::ItemKind::VARIANTS {
            let mut row = item_row(1);
            row.item_ref.kind = *kind;
            put(row);
        }
        for reason in domain::turn::Reason::VARIANTS {
            put(ItemRow {
                turn: Turn::Me(*reason),
                ..item_row(1)
            });
        }
        for decision in ReviewDecision::VARIANTS {
            put(ItemRow {
                review_decision: Some(*decision),
                ..item_row(1)
            });
        }
        for ci in CiState::VARIANTS {
            put(ItemRow {
                ci_state: Some(*ci),
                ..item_row(1)
            });
        }
        for kind in ActorKind::VARIANTS {
            let mut row = item_row(1);
            row.author.kind = *kind;
            put(row);
        }
        for kind in [
            "comment",
            "review",
            "review_requested",
            "assigned",
            "pushed",
        ] {
            put(ItemRow {
                last_human: Some(LastHuman {
                    at: Timestamp(1),
                    login: Login::parse("octocat").expect("login"),
                    kind: kind.to_owned(),
                    url: None,
                }),
                ..item_row(1)
            });
        }
    }

    #[test]
    fn closed_item_needs_a_closed_at() {
        let conn = memory_db();
        let row = ItemRow {
            state: ItemState::Merged,
            closed_at: None,
            ..item_row(1)
        };
        assert!(exec(&conn, &upsert(&row)).is_err());
    }

    #[test]
    fn hostile_title_is_stored_verbatim() {
        for title in HOSTILE_TITLES {
            let bound = memory_db();
            let row = ItemRow {
                title: title.to_owned(),
                ..item_row(1)
            };
            exec(&bound, &upsert(&row)).expect("bound insert");
            assert_eq!(record(&bound, &row.node_id).title, title);
            assert_eq!(count_rows(&bound, "sqlite_master WHERE type = 'table'"), 7);

            let dumped = memory_db();
            let literal = upsert(&row).to_literal_sql().expect("literal");
            assert!(!literal.contains('\n'), "one statement per line");
            dumped.execute_batch(&literal).expect("literal insert");
            assert_eq!(record(&dumped, &row.node_id).title, title);
            assert_eq!(count_rows(&dumped, "items"), 1);
            assert_eq!(count_rows(&dumped, "sqlite_master WHERE type = 'table'"), 7);
        }
    }

    #[derive(Deserialize)]
    struct DomainFixture {
        me: Login,
        item: Item,
        #[serde(rename = "expect")]
        _expect: IgnoredAny,
    }

    #[test]
    fn every_domain_fixture_round_trips() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../domain/tests/fixtures");
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).expect("fixtures dir") {
            let path = entry.expect("entry").path();
            let text = std::fs::read_to_string(&path).expect("read");
            let fixture: DomainFixture = serde_json::from_str(&text).expect("parse");
            let classification = classify(&fixture.item, &fixture.me);
            let meta = ItemMeta::placeholder(&fixture.item.item_ref);
            let node_id = meta.node_id.clone();
            let row = ItemRow::from_classified(
                meta,
                &fixture.item,
                &fixture.me,
                &classification,
                Timestamp(5000),
            );
            let conn = memory_db();
            exec(&conn, &upsert(&row)).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let back = ItemRow::try_from(record(&conn, &node_id)).expect("read back");
            let name = path.display();
            assert_eq!(back.turn, classification.turn, "{name}");
            assert_eq!(back.stage, classification.stage, "{name}");
            assert_eq!(back.since, classification.since, "{name}");
            assert_eq!(back, row, "{name}");
            seen += 1;
        }
        assert!(seen > 0, "no fixtures found in {}", dir.display());
    }
}
