-- Optional closed-network peer filtering. Default-open unless closed_network=1.

ALTER TABLE instance_settings ADD COLUMN closed_network INTEGER NOT NULL DEFAULT 0
    CHECK(closed_network IN (0, 1));

CREATE TABLE IF NOT EXISTS federation_allowlist (
    host TEXT PRIMARY KEY,
    note TEXT,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_federation_allowlist_enabled
    ON federation_allowlist(enabled);
