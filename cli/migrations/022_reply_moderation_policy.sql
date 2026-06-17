ALTER TABLE moderation_settings
    ADD COLUMN reply_policy TEXT NOT NULL DEFAULT 'warn'
    CHECK (reply_policy IN ('off', 'warn', 'review', 'hide', 'reject'));

ALTER TABLE moderation_settings
    ADD COLUMN ai_enabled BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE moderation_settings
    ADD COLUMN ai_model TEXT DEFAULT '@cf/meta/llama-guard-3-8b';

ALTER TABLE moderation_settings
    ADD COLUMN ai_daily_budget INTEGER NOT NULL DEFAULT 0;
