-- Authentication tokens table
-- Stores refresh tokens for revocation capability

CREATE TABLE IF NOT EXISTS auth_tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL,
    refresh_token TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked_at INTEGER,
    device_name TEXT,
    last_used_at INTEGER
);

CREATE INDEX idx_auth_tokens_refresh ON auth_tokens(refresh_token);
CREATE INDEX idx_auth_tokens_username ON auth_tokens(username);
CREATE INDEX idx_auth_tokens_expires ON auth_tokens(expires_at);

-- API tokens table (for long-lived API access)
CREATE TABLE IF NOT EXISTS api_tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    scopes TEXT NOT NULL DEFAULT 'read,write',
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    last_used_at INTEGER,
    revoked_at INTEGER
);

CREATE INDEX idx_api_tokens_hash ON api_tokens(token_hash);
CREATE INDEX idx_api_tokens_username ON api_tokens(username);
