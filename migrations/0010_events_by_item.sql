-- The repo cards join `events` to `items` by item. Without this index each join is a full scan of `events`.
CREATE INDEX idx_events_item ON events (item_node_id, occurred_at);
