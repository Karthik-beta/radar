-- Contribution facts (ADR 0010). These tables are never pruned, so none holds a title, a body, or comment text.
-- `external` is 1 when the repo owner is not my login. `source` is 'live' from Enrich or 'imported' from the history import.

CREATE TABLE pr_facts (
  node_id TEXT PRIMARY KEY NOT NULL,
  repo TEXT NOT NULL,
  external INTEGER NOT NULL CHECK (external IN (0, 1)),
  number INTEGER NOT NULL CHECK (number > 0),
  created_at INTEGER NOT NULL,
  merged_at INTEGER,
  closed_at INTEGER,
  first_response_at INTEGER,
  first_response_login TEXT,
  changes_requested_rounds INTEGER NOT NULL CHECK (changes_requested_rounds >= 0),
  ci_failures INTEGER NOT NULL CHECK (ci_failures >= 0),
  additions INTEGER NOT NULL CHECK (additions >= 0),
  deletions INTEGER NOT NULL CHECK (deletions >= 0),
  changed_files INTEGER NOT NULL CHECK (changed_files >= 0),
  agent INTEGER NOT NULL CHECK (agent IN (0, 1)),
  source TEXT NOT NULL CHECK (source IN ('live', 'imported')),
  CHECK ((first_response_at IS NULL) = (first_response_login IS NULL))
) STRICT;

CREATE INDEX idx_pr_facts_created_at ON pr_facts (created_at);
CREATE INDEX idx_pr_facts_merged_at ON pr_facts (merged_at);

CREATE TABLE review_facts (
  review_id TEXT PRIMARY KEY NOT NULL,
  repo TEXT NOT NULL,
  external INTEGER NOT NULL CHECK (external IN (0, 1)),
  pr_number INTEGER NOT NULL CHECK (pr_number > 0),
  pr_author_login TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('approved', 'changes_requested', 'commented', 'dismissed')),
  submitted_at INTEGER NOT NULL,
  source TEXT NOT NULL CHECK (source IN ('live', 'imported'))
) STRICT;

CREATE INDEX idx_review_facts_submitted_at ON review_facts (submitted_at);

-- One row per change of stage or turn on a pull request I authored. Rows are only inserted.
CREATE TABLE pr_transitions (
  id INTEGER PRIMARY KEY,
  item_node_id TEXT NOT NULL,
  stage TEXT NOT NULL CHECK (stage IN (
    'draft', 'ci_running', 'ci_failing', 'awaiting_review',
    'changes_requested', 'approved', 'merged', 'closed'
  )),
  turn TEXT NOT NULL CHECK (turn IN ('me', 'them')),
  at INTEGER NOT NULL,
  UNIQUE (item_node_id, at, stage, turn)
) STRICT;

CREATE INDEX idx_pr_transitions_at ON pr_transitions (at);

CREATE TABLE pr_issue_links (
  pr_node_id TEXT NOT NULL,
  issue_repo TEXT NOT NULL,
  issue_number INTEGER NOT NULL CHECK (issue_number > 0),
  issue_authored_by_me INTEGER NOT NULL CHECK (issue_authored_by_me IN (0, 1)),
  issue_commented_by_me INTEGER NOT NULL CHECK (issue_commented_by_me IN (0, 1)),
  PRIMARY KEY (pr_node_id, issue_repo, issue_number)
) STRICT;

-- Events by me, and `ci_failed` events on my pull requests, that the weekly rollup folded.
-- The columns and CHECKs match `events`, so the rollup copies a row with `SELECT *`.
CREATE TABLE own_events (
  id TEXT PRIMARY KEY NOT NULL,
  item_node_id TEXT NOT NULL,
  repo TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN (
    'opened', 'commented', 'reviewed', 'review_requested', 'assigned', 'mentioned',
    'committed', 'merged', 'closed', 'reopened', 'ci_failed', 'ci_passed'
  )),
  review_state TEXT CHECK (review_state IN (
    'approved', 'changes_requested', 'commented', 'dismissed', 'pending'
  )),
  actor_login TEXT,
  actor_is_me INTEGER NOT NULL CHECK (actor_is_me IN (0, 1)),
  actor_is_bot INTEGER NOT NULL CHECK (actor_is_bot IN (0, 1)),
  occurred_at INTEGER NOT NULL,
  source TEXT NOT NULL CHECK (source IN ('poll', 'backfill', 'webhook', 'agent')),
  ingested_at INTEGER NOT NULL,
  CHECK (review_state IS NULL OR kind = 'reviewed'),
  CHECK (actor_login IS NOT NULL OR kind IN ('ci_failed', 'ci_passed'))
) STRICT;

CREATE INDEX idx_own_events_occurred_at ON own_events (occurred_at);
CREATE INDEX idx_own_events_item ON own_events (item_node_id, occurred_at);

CREATE TABLE import_state (
  id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
  facts_imported_at INTEGER,
  account_created_at INTEGER
) STRICT;

INSERT INTO import_state (id) VALUES (1);

ALTER TABLE repos ADD COLUMN owned_by_me INTEGER NOT NULL DEFAULT 0 CHECK (owned_by_me IN (0, 1));
ALTER TABLE repos ADD COLUMN stars INTEGER CHECK (stars IS NULL OR stars >= 0);
ALTER TABLE repos ADD COLUMN primary_language TEXT;
ALTER TABLE repos ADD COLUMN meta_refreshed_at INTEGER;

ALTER TABLE settings ADD COLUMN utc_offset_minutes INTEGER NOT NULL DEFAULT 0 CHECK (utc_offset_minutes BETWEEN -720 AND 840);
ALTER TABLE settings ADD COLUMN goal_merged_per_month INTEGER CHECK (goal_merged_per_month IS NULL OR goal_merged_per_month > 0);
