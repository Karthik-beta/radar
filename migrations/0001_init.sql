-- All timestamps are epoch seconds. Booleans are 0 or 1.
-- Radar stores metadata only: no comment bodies, no unread flag.

CREATE TABLE notification_threads (
  id TEXT PRIMARY KEY NOT NULL,
  repo TEXT NOT NULL,
  subject_kind TEXT NOT NULL CHECK (subject_kind IN ('pr', 'issue', 'other')),
  subject_title TEXT NOT NULL,
  subject_number INTEGER CHECK (subject_number IS NULL OR subject_number > 0),
  item_node_id TEXT,
  reason TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  synced_at INTEGER NOT NULL,
  CHECK (subject_kind = 'other' OR subject_number IS NOT NULL)
) STRICT;

CREATE TABLE items (
  node_id TEXT PRIMARY KEY NOT NULL,
  repo TEXT NOT NULL,
  number INTEGER NOT NULL CHECK (number > 0),
  kind TEXT NOT NULL CHECK (kind IN ('pr', 'issue')),
  title TEXT NOT NULL,
  url TEXT NOT NULL,
  head_ref TEXT,
  author_login TEXT NOT NULL,
  author_kind TEXT NOT NULL CHECK (author_kind IN ('user', 'bot')),
  authored_by_me INTEGER NOT NULL CHECK (authored_by_me IN (0, 1)),
  state TEXT NOT NULL CHECK (state IN ('open', 'draft', 'merged', 'closed')),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  closed_at INTEGER,
  turn TEXT NOT NULL CHECK (turn IN ('me', 'them')),
  reason TEXT CHECK (reason IN (
    'review_requested', 'changes_requested', 'ci_failed', 'mentioned', 'assigned', 'replied'
  )),
  since INTEGER NOT NULL,
  stage TEXT CHECK (stage IN (
    'draft', 'ci_running', 'ci_failing', 'awaiting_review',
    'changes_requested', 'approved', 'merged', 'closed'
  )),
  ci_state TEXT CHECK (ci_state IN ('pending', 'passing', 'failing')),
  review_decision TEXT CHECK (review_decision IN (
    'approved', 'changes_requested', 'review_required'
  )),
  last_human_at INTEGER,
  last_human_login TEXT,
  last_human_kind TEXT CHECK (last_human_kind IN (
    'comment', 'review', 'review_requested', 'assigned', 'pushed'
  )),
  last_human_url TEXT,
  synced_at INTEGER NOT NULL,
  UNIQUE (repo, number),
  CHECK (
    (turn = 'me' AND reason IS NOT NULL) OR (turn = 'them' AND reason IS NULL)
  ),
  CHECK (
    (last_human_at IS NULL) = (last_human_login IS NULL)
    AND (last_human_at IS NULL) = (last_human_kind IS NULL)
  ),
  CHECK (state IN ('open', 'draft') OR closed_at IS NOT NULL)
) STRICT;

CREATE INDEX idx_items_repo ON items (repo);

CREATE INDEX idx_items_me_since ON items (since)
  WHERE turn = 'me' AND state IN ('open', 'draft');

CREATE INDEX idx_items_them_since ON items (since)
  WHERE turn = 'them' AND authored_by_me = 1 AND state IN ('open', 'draft');

CREATE INDEX idx_items_pipeline ON items (stage, since)
  WHERE kind = 'pr' AND authored_by_me = 1 AND state IN ('open', 'draft');

CREATE INDEX idx_items_closed ON items (closed_at)
  WHERE state IN ('merged', 'closed');

CREATE TABLE events (
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

CREATE INDEX idx_events_occurred_at ON events (occurred_at);
CREATE INDEX idx_events_repo_occurred_at ON events (repo, occurred_at);

CREATE TABLE repos (
  full_name TEXT PRIMARY KEY NOT NULL,
  open_prs INTEGER NOT NULL CHECK (open_prs >= 0),
  merged_prs INTEGER NOT NULL CHECK (merged_prs >= 0),
  closed_prs INTEGER NOT NULL CHECK (closed_prs >= 0),
  median_first_response_hours REAL CHECK (median_first_response_hours >= 0),
  last_activity_at INTEGER,
  updated_at INTEGER NOT NULL
) STRICT;

-- week_start is Monday 00:00 UTC. 1970-01-05, the first Monday, is 345600 s after the epoch.
CREATE TABLE rollups_weekly (
  week_start INTEGER PRIMARY KEY NOT NULL CHECK (week_start % 604800 = 345600),
  prs_opened INTEGER NOT NULL CHECK (prs_opened >= 0),
  prs_merged INTEGER NOT NULL CHECK (prs_merged >= 0),
  repos_touched INTEGER NOT NULL CHECK (repos_touched >= 0),
  events INTEGER NOT NULL CHECK (events >= 0)
) STRICT;

-- value is last_human_at at dismissal for 'dismissed' (0 when there was none), the wake time for 'snoozed'.
CREATE TABLE local_state (
  item_id TEXT PRIMARY KEY NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('dismissed', 'snoozed')),
  value INTEGER NOT NULL
) STRICT;

CREATE TABLE auth (
  id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
  github_user_id INTEGER NOT NULL,
  login TEXT NOT NULL,
  token_cipher BLOB,
  token_nonce BLOB,
  scopes TEXT NOT NULL,
  scopes_checked_at INTEGER,
  session_epoch INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  CHECK ((token_cipher IS NULL) = (token_nonce IS NULL))
) STRICT;
