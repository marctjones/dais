-- Add moderation fields to replies table
ALTER TABLE replies ADD COLUMN moderation_status TEXT DEFAULT 'approved' CHECK(moderation_status IN ('approved', 'pending', 'hidden', 'rejected'));
ALTER TABLE replies ADD COLUMN moderation_score REAL DEFAULT 0.0;  -- 0.0 = clean, 1.0 = toxic
ALTER TABLE replies ADD COLUMN moderation_flags TEXT;  -- JSON array of detected issues
ALTER TABLE replies ADD COLUMN moderation_checked_at TEXT;
ALTER TABLE replies ADD COLUMN hidden BOOLEAN DEFAULT FALSE;  -- Quick flag for display

-- Add indexes for moderation queries
CREATE INDEX IF NOT EXISTS idx_replies_moderation_status ON replies(moderation_status);
CREATE INDEX IF NOT EXISTS idx_replies_hidden ON replies(hidden);

-- Moderation settings table
CREATE TABLE IF NOT EXISTS moderation_settings (
    id INTEGER PRIMARY KEY DEFAULT 1,
    auto_hide_threshold REAL DEFAULT 0.7,  -- Hide if score > 0.7
    auto_reject_threshold REAL DEFAULT 0.9,  -- Reject if score > 0.9
    enabled BOOLEAN DEFAULT TRUE,
    check_sentiment BOOLEAN DEFAULT TRUE,
    check_toxicity BOOLEAN DEFAULT TRUE,
    notify_on_flagged BOOLEAN DEFAULT TRUE,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    CHECK (id = 1)  -- Only one row allowed
);

-- Insert default settings
INSERT OR IGNORE INTO moderation_settings (id) VALUES (1);
