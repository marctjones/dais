-- Persist ATProto sync commit metadata handled through dais-core.
CREATE TABLE IF NOT EXISTS atproto_sync_commits (
    id TEXT PRIMARY KEY,
    repo_did TEXT NOT NULL,
    commit_cid TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'received'
        CHECK(status IN ('received', 'queued', 'applied', 'failed')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(repo_did, commit_cid)
);

CREATE INDEX IF NOT EXISTS idx_atproto_sync_commits_repo_sequence
    ON atproto_sync_commits(repo_did, sequence);

CREATE INDEX IF NOT EXISTS idx_atproto_sync_commits_status
    ON atproto_sync_commits(status, updated_at);
