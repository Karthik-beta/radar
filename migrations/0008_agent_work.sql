-- Agent work before a pull request exists (R23). Signed pushes to agent/*
-- branches and agent check-ins upsert one row per key. Metadata only.
CREATE TABLE agent_work (
  key TEXT PRIMARY KEY NOT NULL,
  repo TEXT NOT NULL,
  branch TEXT,
  issue INTEGER CHECK (issue IS NULL OR issue > 0),
  agent TEXT,
  reported TEXT NOT NULL CHECK (reported IN ('working', 'blocked', 'finished')),
  title TEXT,
  first_seen_at INTEGER NOT NULL,
  last_signal_at INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_agent_work_signal ON agent_work (last_signal_at DESC);
