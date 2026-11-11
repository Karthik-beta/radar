-- 'seeded' rows come from GitHub's contribution history, for weeks before the backfill window. A rolled row replaces a seeded one.
ALTER TABLE rollups_weekly ADD COLUMN source TEXT NOT NULL DEFAULT 'rolled' CHECK (source IN ('rolled', 'seeded'));
CREATE INDEX idx_items_authored_created ON items (created_at) WHERE kind = 'pr' AND authored_by_me = 1;
