-- One row per OAuth state a callback has spent, kept until the state cookie would expire.
-- The state cookie alone cannot stop a replay of a captured callback URL and its cookie.
CREATE TABLE used_oauth_states (
  state_hash BLOB PRIMARY KEY NOT NULL CHECK (length(state_hash) = 32),
  expires_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
