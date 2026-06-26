ALTER TABLE replies
    ADD COLUMN visibility TEXT NOT NULL DEFAULT 'public'
    CHECK(visibility IN ('public', 'unlisted', 'followers', 'direct'));
