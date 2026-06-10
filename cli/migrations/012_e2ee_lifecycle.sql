-- E2EE product lifecycle: persist encryptedMessage envelopes for read links.

ALTER TABLE posts ADD COLUMN encrypted_message TEXT;
ALTER TABLE timeline_posts ADD COLUMN encrypted_message TEXT;

CREATE INDEX IF NOT EXISTS idx_posts_encrypted ON posts(encrypted_message);
CREATE INDEX IF NOT EXISTS idx_timeline_posts_encrypted ON timeline_posts(encrypted_message);
