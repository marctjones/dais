-- v0.17 rich ActivityPub object support.
-- Adds managed actor type selection and Event metadata while preserving
-- existing Person/Note defaults.

ALTER TABLE actors ADD COLUMN actor_type TEXT NOT NULL DEFAULT 'Person' CHECK(actor_type IN ('Person', 'Group', 'Organization'));

ALTER TABLE posts ADD COLUMN start_time TEXT;
ALTER TABLE posts ADD COLUMN end_time TEXT;
ALTER TABLE posts ADD COLUMN location TEXT;

CREATE INDEX IF NOT EXISTS idx_posts_event_start ON posts(start_time) WHERE object_type = 'Event';
