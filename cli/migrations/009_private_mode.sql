-- Private Mode M1: instance-wide privacy defaults.

CREATE TABLE IF NOT EXISTS instance_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    default_visibility TEXT NOT NULL DEFAULT 'followers'
        CHECK(default_visibility IN ('public', 'unlisted', 'followers', 'direct')),
    require_authorized_fetch INTEGER NOT NULL DEFAULT 1
        CHECK(require_authorized_fetch IN (0, 1)),
    manually_approves_followers INTEGER NOT NULL DEFAULT 1
        CHECK(manually_approves_followers IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO instance_settings (
    id,
    default_visibility,
    require_authorized_fetch,
    manually_approves_followers
) VALUES (
    1,
    'followers',
    1,
    1
);
