-- E2EE v2-only cleanup.
-- Migration: 033_e2ee_v2_only
-- Created: 2026-07-08
--
-- Dais no longer supports encryptedMessage v1/RSA fallback messages or devices.
-- Keep only MLS/RFC 9420 device material, conversations, and messages.

DELETE FROM e2ee_mls_message_metadata
WHERE message_id IN (
    SELECT m.id
    FROM e2ee_messages m
    JOIN e2ee_conversations c ON c.id = m.conversation_id
    WHERE c.protocol != 'mls-rfc9420'
       OR CASE
            WHEN json_valid(m.ciphertext) THEN
                json_extract(m.ciphertext, '$.v') != 2
                OR json_extract(m.ciphertext, '$.protocol') != 'mls-rfc9420'
            ELSE 1
          END
);

DELETE FROM e2ee_messages
WHERE id IN (
    SELECT m.id
    FROM e2ee_messages m
    JOIN e2ee_conversations c ON c.id = m.conversation_id
    WHERE c.protocol != 'mls-rfc9420'
       OR CASE
            WHEN json_valid(m.ciphertext) THEN
                json_extract(m.ciphertext, '$.v') != 2
                OR json_extract(m.ciphertext, '$.protocol') != 'mls-rfc9420'
            ELSE 1
          END
);

DELETE FROM e2ee_conversations
WHERE protocol != 'mls-rfc9420'
   OR id NOT IN (SELECT DISTINCT conversation_id FROM e2ee_messages);

DELETE FROM e2ee_devices
WHERE protocol != 'mls-rfc9420';

DELETE FROM e2ee_peer_devices
WHERE protocol != 'mls-rfc9420';
