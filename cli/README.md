# `cli/` — D1 schema migrations

The Python operator CLI that used to live here was **removed** once the native Rust
client (`/client`) reached ActivityPub parity. Recover it from git history or tag
`v1.4.0` if ever needed.

What remains here is the **authoritative Cloudflare D1 schema** — the numbered SQL
migrations applied to the `dais-social` database, which the Rust client's queries
target:

```
migrations/
├── 001_initial_schema.sql   actors, followers, following, posts, activities
├── 002_interactions.sql     replies, interactions, notifications
├── 003_moderation.sql
├── 004_blocking.sql         blocks
├── 005_following.sql        following (target_actor_id / target_inbox)
├── 006_protocol_support.sql posts.protocol, atproto_uri/cid
├── 006_auth_tokens.sql      sessions
├── 007_direct_messages.sql  conversations, direct_messages
└── 008_bluesky_chat.sql
```

Apply with, e.g.:

```
wrangler d1 execute DB --remote --file=cli/migrations/001_initial_schema.sql
```
