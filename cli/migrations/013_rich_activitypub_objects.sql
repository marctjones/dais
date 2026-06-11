-- Rich ActivityPub object metadata for Article, Document, Event, and later types.
-- Existing posts default to Note and keep their current federation behavior.

ALTER TABLE posts ADD COLUMN object_type TEXT NOT NULL DEFAULT 'Note';
ALTER TABLE posts ADD COLUMN name TEXT;
ALTER TABLE posts ADD COLUMN summary TEXT;

CREATE INDEX IF NOT EXISTS idx_posts_object_type ON posts(object_type);
