-- Generic ActivityPub delivery payloads for non-Create activities.
-- Create deliveries continue to rebuild from posts when activity_json is NULL.

ALTER TABLE deliveries ADD COLUMN activity_type TEXT DEFAULT 'Create';
ALTER TABLE deliveries ADD COLUMN activity_json TEXT;

CREATE INDEX IF NOT EXISTS idx_deliveries_activity_type ON deliveries(activity_type);
