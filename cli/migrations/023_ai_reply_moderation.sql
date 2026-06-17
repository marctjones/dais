ALTER TABLE replies
    ADD COLUMN ai_moderation_checked_at TEXT;

ALTER TABLE replies
    ADD COLUMN ai_moderation_result TEXT;
