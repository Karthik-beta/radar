-- The weekly repo refresh reads stars and language with one `nodes` query per 50 repos, so it keeps each repo's node ID.
ALTER TABLE repos ADD COLUMN node_id TEXT;
