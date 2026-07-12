-- E2EE v2-only cleanup.
-- Migration: 033_e2ee_v2_only
-- Created: 2026-07-08
--
-- Dais no longer supports encryptedMessage v1/RSA fallback messages or devices.
-- Keep only MLS/RFC 9420 device material, conversations, and messages.
--
-- Ordering matters. e2ee_mls_message_metadata has no foreign key to
-- e2ee_messages, so its orphans are cleaned up explicitly. Conversations are
-- purged last because e2ee_mls_group_states, e2ee_mls_group_members,
-- e2ee_mls_pending_commits, and e2ee_mls_message_metadata all cascade from
-- e2ee_conversations(id).

-- 1. Drop every message that is not a v2 MLS envelope. The ciphertext predicate
--    stands on its own rather than joining e2ee_conversations, so messages
--    orphaned from a missing conversation row are purged too instead of
--    surviving the sweep.
DELETE FROM e2ee_messages
WHERE CASE
        WHEN json_valid(ciphertext) THEN
            json_extract(ciphertext, '$.v') != 2
            OR json_extract(ciphertext, '$.protocol') != 'mls-rfc9420'
        ELSE 1
      END
   OR conversation_id NOT IN (
        SELECT id FROM e2ee_conversations WHERE protocol = 'mls-rfc9420'
      );

-- 2. Clear MLS metadata left pointing at messages that no longer exist.
DELETE FROM e2ee_mls_message_metadata
WHERE message_id NOT IN (SELECT id FROM e2ee_messages);

-- 3. Purge non-MLS conversations only.
--
--    Deliberately NOT deleting conversations that have zero messages. An MLS
--    group is established (epoch + serialized group state persisted) before any
--    application message is sent, and e2ee_mls_group_states cascades from this
--    table. Deleting a message-less conversation would destroy the group state
--    of a live conversation and leave its history unrecoverable.
DELETE FROM e2ee_conversations
WHERE protocol != 'mls-rfc9420';

-- 4. Purge non-MLS device material.
DELETE FROM e2ee_devices
WHERE protocol != 'mls-rfc9420';

DELETE FROM e2ee_peer_devices
WHERE protocol != 'mls-rfc9420';
