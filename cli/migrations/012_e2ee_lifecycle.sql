-- Historical E2EE product lifecycle: persisted legacy encrypted-message
-- envelopes for read links. Current owner E2EE uses MLS/RFC 9420 v2.

ALTER TABLE posts ADD COLUMN encrypted_message TEXT;
ALTER TABLE timeline_posts ADD COLUMN encrypted_message TEXT;

CREATE INDEX IF NOT EXISTS idx_posts_encrypted ON posts(encrypted_message);
CREATE INDEX IF NOT EXISTS idx_timeline_posts_encrypted ON timeline_posts(encrypted_message);
