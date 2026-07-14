-- Caches each followed DID's resolved PDS host, so the Jetstream-based
-- firehose consumer (issue #377) doesn't re-resolve a DID document on every
-- single event. Bluesky's relay does not implement com.atproto.sync.getRecord
-- itself (only PDS instances do), so this resolution step is required before
-- verifying any record.
CREATE TABLE IF NOT EXISTS atproto_pds_cache (
    did TEXT PRIMARY KEY,
    pds_endpoint TEXT NOT NULL,
    resolved_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
