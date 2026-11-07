ALTER TABLE items ADD COLUMN requested_reviewers TEXT NOT NULL DEFAULT '[]';
ALTER TABLE items ADD COLUMN stage_since INTEGER;
ALTER TABLE items ADD COLUMN latest_url TEXT;
