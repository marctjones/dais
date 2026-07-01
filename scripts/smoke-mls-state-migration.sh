#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DB_PATH="${TMPDIR:-/tmp}/dais-mls-state-migration-smoke.sqlite"

rm -f "$DB_PATH"

result="$(sqlite3 "$DB_PATH" <<SQL
.read $ROOT_DIR/cli/migrations/026_e2ee_devices.sql
.read $ROOT_DIR/cli/migrations/029_mls_state.sql

PRAGMA foreign_keys = ON;

INSERT INTO e2ee_conversations (
    id,
    protocol,
    participants,
    epoch,
    state
) VALUES (
    'e2ee-conversation-mls-smoke',
    'dais-mls-v1',
    '["https://social.dais.social/users/social","https://social.skpt.cl/users/social"]',
    '1',
    'legacy-v1-state'
);

INSERT INTO e2ee_messages (
    id,
    conversation_id,
    sender_actor_id,
    sender_device_id,
    ciphertext,
    aad
) VALUES (
    'https://social.dais.social/users/social/e2ee/messages/mls-smoke',
    'e2ee-conversation-mls-smoke',
    'https://social.dais.social/users/social',
    'dais-mac',
    'legacy-v1-ciphertext',
    '{"v":1}'
);

INSERT INTO e2ee_mls_group_states (
    id,
    conversation_id,
    local_actor_id,
    local_device_id,
    group_id,
    epoch,
    serialized_group_state,
    state_checksum
) VALUES (
    'mls-state-dais-mac',
    'e2ee-conversation-mls-smoke',
    'https://social.dais.social/users/social',
    'dais-mac',
    'mls-group-smoke',
    7,
    'serialized-openmls-state',
    'sha256:state'
);

INSERT INTO e2ee_mls_group_members (
    id,
    group_state_id,
    actor_id,
    device_id,
    credential,
    key_package,
    fingerprint,
    leaf_index,
    member_status,
    added_epoch,
    trusted_at
) VALUES (
    'mls-member-dais-mac',
    'mls-state-dais-mac',
    'https://social.dais.social/users/social',
    'dais-mac',
    'credential',
    'key-package',
    'sha256:device',
    0,
    'active',
    0,
    datetime('now')
);

INSERT INTO e2ee_mls_group_members (
    id,
    group_state_id,
    actor_id,
    device_id,
    credential,
    key_package,
    fingerprint,
    leaf_index,
    member_status,
    added_epoch,
    removed_epoch,
    revoked_at
) VALUES (
    'mls-member-skpt-phone',
    'mls-state-dais-mac',
    'https://social.skpt.cl/users/social',
    'skpt-phone',
    'credential',
    'key-package',
    'sha256:peer',
    1,
    'revoked',
    1,
    6,
    datetime('now')
);

INSERT INTO e2ee_mls_pending_commits (
    id,
    group_state_id,
    epoch,
    sender_actor_id,
    sender_device_id,
    commit_message,
    status
) VALUES (
    'mls-commit-8',
    'mls-state-dais-mac',
    8,
    'https://social.dais.social/users/social',
    'dais-mac',
    'serialized-commit',
    'pending'
);

INSERT INTO e2ee_mls_message_metadata (
    message_id,
    conversation_id,
    group_state_id,
    group_id,
    epoch,
    sender_actor_id,
    sender_device_id,
    decrypt_status
) VALUES
    (
        'https://social.dais.social/users/social/e2ee/messages/missing-state',
        'e2ee-conversation-mls-smoke',
        NULL,
        'mls-group-smoke',
        7,
        'https://social.skpt.cl/users/social',
        'skpt-phone',
        'missing_state'
    ),
    (
        'https://social.dais.social/users/social/e2ee/messages/stale-epoch',
        'e2ee-conversation-mls-smoke',
        'mls-state-dais-mac',
        'mls-group-smoke',
        12,
        'https://social.skpt.cl/users/social',
        'skpt-phone',
        'stale_epoch'
    ),
    (
        'https://social.dais.social/users/social/e2ee/messages/revoked-device',
        'e2ee-conversation-mls-smoke',
        'mls-state-dais-mac',
        'mls-group-smoke',
        7,
        'https://social.skpt.cl/users/social',
        'skpt-phone',
        'revoked_device'
    );

SELECT CASE
    WHEN (SELECT COUNT(*) FROM e2ee_messages) = 1
     AND (SELECT COUNT(*) FROM e2ee_mls_group_states) = 1
     AND (SELECT COUNT(*) FROM e2ee_mls_group_members WHERE member_status = 'revoked') = 1
     AND (SELECT COUNT(*) FROM e2ee_mls_pending_commits WHERE status = 'pending') = 1
     AND (SELECT COUNT(*) FROM e2ee_mls_message_metadata WHERE decrypt_status = 'missing_state') = 1
     AND (SELECT COUNT(*) FROM e2ee_mls_message_metadata WHERE decrypt_status = 'stale_epoch') = 1
     AND (SELECT COUNT(*) FROM e2ee_mls_message_metadata WHERE decrypt_status = 'revoked_device') = 1
    THEN 'ok'
    ELSE 'failed'
END;
SQL
)"

if [[ "$result" != "ok" ]]; then
    echo "MLS state migration smoke failed: $result" >&2
    exit 1
fi

rm -f "$DB_PATH"
echo "MLS state migration smoke passed"
