-- Personal AppView read-floor indexes for AT Protocol compatibility.
-- These are intentionally additive and safe to apply after earlier schema
-- migrations on both local and remote D1 databases.

CREATE INDEX IF NOT EXISTS idx_posts_public_published
ON posts(visibility, published_at DESC);

CREATE INDEX IF NOT EXISTS idx_notifications_created_desc
ON notifications(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_interactions_object_type
ON interactions(object_url, type, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_followers_actor_status_created
ON followers(actor_id, status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_following_actor_status_created
ON following(actor_id, status, created_at DESC);
