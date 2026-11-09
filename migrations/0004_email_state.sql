-- The last GitHub notification email Radar received, as a single row. Only the time and
-- the X-GitHub-Reason word are kept. Radar stores metadata only, never the message.
CREATE TABLE email_state (
  id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
  last_email_at INTEGER NOT NULL,
  last_reason TEXT NOT NULL
) STRICT;
