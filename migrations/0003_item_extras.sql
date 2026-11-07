-- requested_reviewers is a JSON array of logins. stage_since is when the stage last changed.
-- latest_url links the newest timeline node. Rows that predate this migration start from items.since and items.url.
ALTER TABLE items ADD COLUMN requested_reviewers TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(requested_reviewers));
ALTER TABLE items ADD COLUMN stage_since INTEGER NOT NULL DEFAULT 0;
ALTER TABLE items ADD COLUMN latest_url TEXT NOT NULL DEFAULT '';
UPDATE items SET stage_since = since, latest_url = url;
