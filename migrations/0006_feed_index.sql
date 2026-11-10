-- The feed reads events newest first, with an optional repo filter. These two
-- indexes serve its keyset order, and replace 0001's ascending pair.
CREATE INDEX idx_events_feed ON events (occurred_at DESC, id DESC);
CREATE INDEX idx_events_repo_feed ON events (repo, occurred_at DESC, id DESC);
DROP INDEX idx_events_repo_occurred_at;
