-- E2EE device and peer trust lifecycle
-- Migration: 026_e2ee_devices
-- Created: 2026-06-22

CREATE TABLE IF NOT EXISTS e2ee_devices (
    id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    display_name TEXT,
    protocol TEXT NOT NULL DEFAULT 'mls-rfc9420',
    credential TEXT NOT NULL,
    key_package TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    UNIQUE(actor_id, device_id)
);

CREATE TABLE IF NOT EXISTS e2ee_peer_devices (
    id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    display_name TEXT,
    protocol TEXT NOT NULL DEFAULT 'mls-rfc9420',
    credential TEXT NOT NULL,
    key_package TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    trust_state TEXT NOT NULL DEFAULT 'untrusted',
    first_seen_at TEXT DEFAULT (datetime('now')),
    last_seen_at TEXT DEFAULT (datetime('now')),
    trusted_at TEXT,
    revoked_at TEXT,
    UNIQUE(actor_id, device_id)
);

CREATE TABLE IF NOT EXISTS e2ee_conversations (
    id TEXT PRIMARY KEY,
    protocol TEXT NOT NULL DEFAULT 'mls-rfc9420',
    participants TEXT NOT NULL,
    epoch TEXT,
    state TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS e2ee_messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    sender_actor_id TEXT NOT NULL,
    sender_device_id TEXT NOT NULL,
    ciphertext TEXT NOT NULL,
    aad TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (conversation_id) REFERENCES e2ee_conversations(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_e2ee_devices_actor_status ON e2ee_devices(actor_id, status);
CREATE INDEX IF NOT EXISTS idx_e2ee_peer_devices_actor_trust ON e2ee_peer_devices(actor_id, trust_state);
CREATE INDEX IF NOT EXISTS idx_e2ee_messages_conversation_created ON e2ee_messages(conversation_id, created_at DESC);
