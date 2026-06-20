-- Owner settings: default post route for native clients.

ALTER TABLE instance_settings ADD COLUMN default_protocol TEXT NOT NULL DEFAULT 'activitypub'
    CHECK(default_protocol IN ('activitypub', 'atproto', 'both'));

