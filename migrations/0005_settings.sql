-- One row of operator settings. Change it with `wrangler d1 execute`, not an endpoint.
-- Nudge thresholds are whole days. The CHECK rejects a pair that would never turn hot.

CREATE TABLE settings (
  id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
  nudge_warm_days INTEGER NOT NULL DEFAULT 7,
  nudge_hot_days INTEGER NOT NULL DEFAULT 14,
  CHECK (nudge_warm_days >= 1 AND nudge_hot_days > nudge_warm_days)
) STRICT;

INSERT INTO settings (id) VALUES (1);
