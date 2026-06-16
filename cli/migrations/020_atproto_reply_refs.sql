-- Preserve AT Protocol reply root/parent refs for PDS-created feed records.
ALTER TABLE posts ADD COLUMN atproto_reply_json TEXT;
