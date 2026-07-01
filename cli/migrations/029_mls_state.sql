-- OpenMLS group state, epochs, pending commits, and recovery metadata.
-- Migration: 029_mls_state
-- Created: 2026-07-01
--
-- This extends the encryptedMessage v1 lifecycle tables from migration 026.
-- The v1 tables remain the durable message/device inventory; these tables add
-- instance-scoped and device-scoped MLS private state for daisEncryptedMessage v2.

CREATE TABLE IF NOT EXISTS e2ee_mls_group_states (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    local_actor_id TEXT NOT NULL,
    local_device_id TEXT NOT NULL,
    group_id TEXT NOT NULL,
    protocol TEXT NOT NULL DEFAULT 'dais-mls-v2',
    epoch INTEGER NOT NULL DEFAULT 0 CHECK(epoch >= 0),
    serialized_group_state TEXT NOT NULL,
    state_checksum TEXT NOT NULL,
    recovery_status TEXT NOT NULL DEFAULT 'available'
        CHECK(recovery_status IN ('available', 'missing', 'stale', 'unrecoverable')),
    recovery_note TEXT NOT NULL DEFAULT 'MLS private state is device-scoped. If this device state is lost, older messages for this group may be unrecoverable.',
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    UNIQUE(conversation_id, local_actor_id, local_device_id),
    UNIQUE(group_id, local_actor_id, local_device_id),
    FOREIGN KEY (conversation_id) REFERENCES e2ee_conversations(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS e2ee_mls_group_members (
    id TEXT PRIMARY KEY,
    group_state_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    credential TEXT NOT NULL,
    key_package TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    leaf_index INTEGER CHECK(leaf_index IS NULL OR leaf_index >= 0),
    member_status TEXT NOT NULL DEFAULT 'active'
        CHECK(member_status IN ('active', 'removed', 'revoked')),
    added_epoch INTEGER NOT NULL DEFAULT 0 CHECK(added_epoch >= 0),
    removed_epoch INTEGER CHECK(removed_epoch IS NULL OR removed_epoch >= added_epoch),
    trusted_at TEXT,
    revoked_at TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    UNIQUE(group_state_id, actor_id, device_id),
    FOREIGN KEY (group_state_id) REFERENCES e2ee_mls_group_states(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS e2ee_mls_pending_commits (
    id TEXT PRIMARY KEY,
    group_state_id TEXT NOT NULL,
    epoch INTEGER NOT NULL CHECK(epoch >= 0),
    sender_actor_id TEXT NOT NULL,
    sender_device_id TEXT NOT NULL,
    commit_message TEXT NOT NULL,
    welcome_message TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'applied', 'rejected')),
    error TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    applied_at TEXT,
    UNIQUE(group_state_id, epoch, sender_actor_id, sender_device_id, status),
    FOREIGN KEY (group_state_id) REFERENCES e2ee_mls_group_states(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS e2ee_mls_message_metadata (
    message_id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    group_state_id TEXT,
    group_id TEXT NOT NULL,
    epoch INTEGER NOT NULL CHECK(epoch >= 0),
    sender_actor_id TEXT NOT NULL,
    sender_device_id TEXT NOT NULL,
    decrypt_status TEXT NOT NULL DEFAULT 'pending'
        CHECK(decrypt_status IN ('pending', 'decrypted', 'missing_state', 'stale_epoch', 'revoked_device', 'failed')),
    error TEXT,
    received_at TEXT DEFAULT (datetime('now')),
    decrypted_at TEXT,
    FOREIGN KEY (conversation_id) REFERENCES e2ee_conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (group_state_id) REFERENCES e2ee_mls_group_states(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_e2ee_mls_group_states_actor_device
    ON e2ee_mls_group_states(local_actor_id, local_device_id, recovery_status);

CREATE INDEX IF NOT EXISTS idx_e2ee_mls_group_members_group_status
    ON e2ee_mls_group_members(group_state_id, member_status);

CREATE INDEX IF NOT EXISTS idx_e2ee_mls_pending_commits_group_epoch
    ON e2ee_mls_pending_commits(group_state_id, epoch, status);

CREATE INDEX IF NOT EXISTS idx_e2ee_mls_message_metadata_conversation_epoch
    ON e2ee_mls_message_metadata(conversation_id, epoch, decrypt_status);
