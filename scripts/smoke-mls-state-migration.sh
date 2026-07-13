#!/usr/bin/env bash
#
# Gate for migration 033 (E2EE v2-only).
#
# 033 rebuilds the E2EE tables so that MLS/RFC 9420 is the only representable
# encrypted-message format. This checks the three things that has to mean:
#
#   1. Legacy v1 material is gone, and so is every v1 envelope on post rows.
#   2. The rebuilt schema still supports the whole MLS lifecycle — group state,
#      active and revoked members, pending commits, and every decrypt_status —
#      so the rebuild did not quietly drop a column or constraint.
#   3. A v1 row cannot be written back. The CHECK constraints, not just the
#      application, refuse it.
#
# The schema is built by replaying every migration in order, the way a real
# instance is built, rather than a hand-picked subset: 033 also clears the
# encrypted_message columns on posts/timeline_posts, which a subset would miss.
#
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DB_PATH="${TMPDIR:-/tmp}/dais-mls-state-migration-smoke.sqlite"
rm -f "$DB_PATH"

fail() {
  printf 'FAIL %s\n' "$1" >&2
  exit 1
}

# Replay the migration chain up to (not including) 033. Some early migrations
# add columns that later ones also add; sqlite reports those and moves on.
for migration in "$ROOT_DIR"/cli/migrations/*.sql; do
  case "$migration" in
    *033_e2ee_v2_only.sql) continue ;;
  esac
  sqlite3 "$DB_PATH" < "$migration" >/dev/null 2>&1 || true
done

for table in posts timeline_posts e2ee_devices e2ee_conversations e2ee_messages e2ee_mls_group_states; do
  sqlite3 "$DB_PATH" "SELECT 1 FROM $table LIMIT 0;" >/dev/null 2>&1 \
    || fail "pre-033 schema is missing $table; the migration chain did not replay"
done

# Seed a realistic pre-033 instance: legacy v1 material next to live v2 material.
sqlite3 "$DB_PATH" <<'SQL' >/dev/null
PRAGMA foreign_keys = ON;

INSERT INTO actors (id, username, public_key, private_key, inbox_url, outbox_url, followers_url, following_url, actor_type)
VALUES ('https://social.dais.social/users/social','social','pk','sk','i','o','f','g','Person');

INSERT INTO e2ee_conversations (id, protocol, participants)
VALUES ('conv-v2','mls-rfc9420','["a","b"]'), ('conv-v1','dais-mls-v1','["a","c"]');

INSERT INTO e2ee_messages (id, conversation_id, sender_actor_id, sender_device_id, ciphertext)
VALUES ('msg-v2','conv-v2','a','mac','{"v":2,"protocol":"mls-rfc9420","ciphertext":"Y2lwaGVy"}'),
       ('msg-v1','conv-v1','a','old','{"v":1,"alg":"RSA-OAEP","ciphertext":"bGVnYWN5"}');

INSERT INTO e2ee_devices (id, actor_id, device_id, protocol, credential, key_package, fingerprint)
VALUES ('dev-v2','a','mac','mls-rfc9420','c','k','f'), ('dev-v1','a','old','dais-mls-v1','c','k','f');

INSERT INTO e2ee_peer_devices (id, actor_id, device_id, protocol, credential, key_package, fingerprint)
VALUES ('peer-v1','b','old','dais-mls-v1','c','k','f');

INSERT INTO posts (id, actor_id, content, visibility, published_at, encrypted_message)
VALUES ('p-v1','https://social.dais.social/users/social','End-to-end encrypted message','public','2026-07-01T00:00:00Z','{"v":1,"alg":"RSA-OAEP"}');

INSERT INTO timeline_posts (object_id, actor_id, content, visibility, published_at, protocol, created_at, encrypted_message)
VALUES ('https://peer/notes/1','https://peer/users/bob','enc','direct','2026-07-01T00:00:00Z','activitypub','2026-07-01T00:00:00Z','{"v":1,"alg":"RSA-OAEP"}');
SQL

seeded="$(sqlite3 "$DB_PATH" "SELECT (SELECT COUNT(*) FROM e2ee_messages) || '/' || (SELECT COUNT(*) FROM posts WHERE encrypted_message IS NOT NULL);")"
[ "$seeded" = "2/1" ] || fail "seed did not take (expected 2 messages / 1 encrypted post, got $seeded)"

# Apply the migration under test.
sqlite3 "$DB_PATH" "PRAGMA foreign_keys=ON;" ".read $ROOT_DIR/cli/migrations/033_e2ee_v2_only.sql" >/dev/null

# 1. Clean slate, and no v1 envelopes left on post rows (the rows themselves stay).
purged="$(sqlite3 "$DB_PATH" <<'SQL'
SELECT CASE
    WHEN (SELECT COUNT(*) FROM e2ee_devices) = 0
     AND (SELECT COUNT(*) FROM e2ee_peer_devices) = 0
     AND (SELECT COUNT(*) FROM e2ee_conversations) = 0
     AND (SELECT COUNT(*) FROM e2ee_messages) = 0
     AND (SELECT COUNT(*) FROM e2ee_mls_message_metadata) = 0
     AND (SELECT COUNT(*) FROM posts WHERE encrypted_message IS NOT NULL) = 0
     AND (SELECT COUNT(*) FROM timeline_posts WHERE encrypted_message IS NOT NULL) = 0
     AND (SELECT COUNT(*) FROM posts) = 1
     AND (SELECT COUNT(*) FROM timeline_posts) = 1
    THEN 'ok' ELSE 'failed'
END;
SQL
)"
[ "$purged" = "ok" ] || fail "033 did not purge legacy E2EE state and v1 post envelopes"
printf 'OK   legacy v1 state purged; post rows kept, their v1 envelopes cleared\n'

# 2. The rebuilt schema still carries the whole MLS lifecycle.
sqlite3 "$DB_PATH" <<'SQL' >/dev/null
PRAGMA foreign_keys = ON;

INSERT INTO e2ee_conversations (id, participants, epoch, state)
VALUES ('conv-mls','["https://social.dais.social/users/social","https://social.skpt.cl/users/social"]','1','mls-state');

INSERT INTO e2ee_messages (id, conversation_id, sender_actor_id, sender_device_id, ciphertext, aad)
VALUES ('msg-mls','conv-mls','https://social.dais.social/users/social','dais-mac',
        '{"v":2,"protocol":"mls-rfc9420","groupId":"bWxzLXNtb2tl","epoch":1,"senderDeviceId":"dais-mac","ciphertext":"Y2lwaGVydGV4dA=="}',
        '{"v":2}');

INSERT INTO e2ee_mls_group_states (id, conversation_id, local_actor_id, local_device_id, group_id, epoch, serialized_group_state, state_checksum)
VALUES ('gs-mac','conv-mls','https://social.dais.social/users/social','dais-mac','mls-group-smoke',7,'serialized-openmls-state','sha256:state');

INSERT INTO e2ee_mls_group_members (id, group_state_id, actor_id, device_id, credential, key_package, fingerprint, leaf_index, member_status, added_epoch, trusted_at)
VALUES ('mem-mac','gs-mac','https://social.dais.social/users/social','dais-mac','credential','key-package','sha256:device',0,'active',0,datetime('now'));

INSERT INTO e2ee_mls_group_members (id, group_state_id, actor_id, device_id, credential, key_package, fingerprint, leaf_index, member_status, added_epoch, removed_epoch, revoked_at)
VALUES ('mem-phone','gs-mac','https://social.skpt.cl/users/social','skpt-phone','credential','key-package','sha256:peer',1,'revoked',1,6,datetime('now'));

INSERT INTO e2ee_mls_pending_commits (id, group_state_id, epoch, sender_actor_id, sender_device_id, commit_message, status)
VALUES ('commit-8','gs-mac',8,'https://social.dais.social/users/social','dais-mac','serialized-commit','pending');

INSERT INTO e2ee_mls_message_metadata (message_id, conversation_id, group_state_id, group_id, epoch, sender_actor_id, sender_device_id, decrypt_status)
VALUES ('m-missing','conv-mls',NULL,'mls-group-smoke',7,'https://social.skpt.cl/users/social','skpt-phone','missing_state'),
       ('m-stale','conv-mls','gs-mac','mls-group-smoke',12,'https://social.skpt.cl/users/social','skpt-phone','stale_epoch'),
       ('m-revoked','conv-mls','gs-mac','mls-group-smoke',7,'https://social.skpt.cl/users/social','skpt-phone','revoked_device');
SQL

lifecycle="$(sqlite3 "$DB_PATH" <<'SQL'
SELECT CASE
    WHEN (SELECT COUNT(*) FROM e2ee_messages) = 1
     AND (SELECT COUNT(*) FROM e2ee_mls_group_states) = 1
     AND (SELECT COUNT(*) FROM e2ee_mls_group_members WHERE member_status = 'active') = 1
     AND (SELECT COUNT(*) FROM e2ee_mls_group_members WHERE member_status = 'revoked') = 1
     AND (SELECT COUNT(*) FROM e2ee_mls_pending_commits WHERE status = 'pending') = 1
     AND (SELECT COUNT(*) FROM e2ee_mls_message_metadata WHERE decrypt_status = 'missing_state') = 1
     AND (SELECT COUNT(*) FROM e2ee_mls_message_metadata WHERE decrypt_status = 'stale_epoch') = 1
     AND (SELECT COUNT(*) FROM e2ee_mls_message_metadata WHERE decrypt_status = 'revoked_device') = 1
    THEN 'ok' ELSE 'failed'
END;
SQL
)"
[ "$lifecycle" = "ok" ] || fail "rebuilt schema cannot carry the MLS lifecycle (group state, members, commits, metadata)"
printf 'OK   rebuilt schema carries MLS group state, active/revoked members, commits, decrypt statuses\n'

# 3. v1 cannot come back. Each of these must be refused by a CHECK constraint.
rejects() {
  local label="$1" sql="$2"
  if sqlite3 "$DB_PATH" "PRAGMA foreign_keys=ON; $sql" >/dev/null 2>&1; then
    fail "database accepted $label; v1 is still representable"
  fi
  printf 'OK   rejected %s\n' "$label"
}

rejects "a v1 device" \
  "INSERT INTO e2ee_devices (id,actor_id,device_id,protocol,credential,key_package,fingerprint) VALUES ('x','a','o','dais-mls-v1','c','k','f');"
rejects "a v1 peer device" \
  "INSERT INTO e2ee_peer_devices (id,actor_id,device_id,protocol,credential,key_package,fingerprint) VALUES ('x','a','o','dais-mls-v1','c','k','f');"
rejects "a v1 conversation" \
  "INSERT INTO e2ee_conversations (id,protocol,participants) VALUES ('x','dais-mls-v1','[]');"
rejects "a v1 RSA message" \
  "INSERT INTO e2ee_messages (id,conversation_id,sender_actor_id,sender_device_id,ciphertext) VALUES ('x','conv-mls','a','d','{\"v\":1,\"alg\":\"RSA-OAEP\",\"ciphertext\":\"bGVnYWN5\"}');"
rejects "a non-JSON ciphertext" \
  "INSERT INTO e2ee_messages (id,conversation_id,sender_actor_id,sender_device_id,ciphertext) VALUES ('x','conv-mls','a','d','not-even-json');"
rejects "a v2 envelope naming a retired protocol" \
  "INSERT INTO e2ee_messages (id,conversation_id,sender_actor_id,sender_device_id,ciphertext) VALUES ('x','conv-mls','a','d','{\"v\":2,\"protocol\":\"dais-mls-v1\",\"ciphertext\":\"Yw==\"}');"

rm -f "$DB_PATH"
echo "MLS state migration smoke passed"
