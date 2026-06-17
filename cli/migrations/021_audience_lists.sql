CREATE TABLE IF NOT EXISTS audience_lists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    allowed_categories TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS audience_list_members (
    list_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (list_id, actor_id)
);

CREATE INDEX IF NOT EXISTS idx_audience_lists_name
    ON audience_lists(name);

CREATE INDEX IF NOT EXISTS idx_audience_list_members_actor
    ON audience_list_members(actor_id);
