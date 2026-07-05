ALTER TABLE audience_lists
    ADD COLUMN group_type TEXT NOT NULL DEFAULT 'audience'
    CHECK(group_type IN ('audience', 'private_group'));

ALTER TABLE audience_lists
    ADD COLUMN membership_visibility TEXT NOT NULL DEFAULT 'private'
    CHECK(membership_visibility IN ('private', 'members', 'public'));

ALTER TABLE audience_lists
    ADD COLUMN posting_policy TEXT NOT NULL DEFAULT 'owner'
    CHECK(posting_policy IN ('owner', 'members'));

CREATE INDEX IF NOT EXISTS idx_audience_lists_group_type
    ON audience_lists(group_type, membership_visibility);
