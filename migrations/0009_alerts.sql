-- One row per event that was sent, failed, or skipped for a reason that cannot change. No row means the event is still to be decided.
-- attempts counts sends, so a crash between a send and its row costs one duplicate message at most.
CREATE TABLE alerts (
  event_id TEXT PRIMARY KEY NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('sent', 'failed', 'skipped')),
  attempts INTEGER NOT NULL CHECK (attempts >= 1),
  updated_at INTEGER NOT NULL
) STRICT;
