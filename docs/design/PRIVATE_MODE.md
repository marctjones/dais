# Design: dais Private Mode

**Status:** Draft / proposal
**Author:** design review, 2026-06
**Scope:** Turn dais from a public-first *publishing node* into a personal,
**private-by-default** social network where you read a feed of your friends and
only your friends can read what you post.

---

## 1. Motivation

dais today is a single-user ActivityPub **publishing node**. Its job is to emit
your posts to the open fediverse and receive interactions (follows, replies,
likes, DMs) into an inbox. It is deliberately *not* a place you go to read a feed
of the people you follow — `VISION.md:67` lists "custom timelines" as a non-goal,
and the only consumption path that exists (`cli/dais_cli/commands/timeline.py`)
polls each followed user's **public** `/outbox` over HTTP.

The desired product is the inverse of that posture:

1. **Private by default** — new posts default to followers-only, not public.
2. **You only see content from friends and people you follow** — a real,
   server-side home timeline built from inbound posts, not ad-hoc outbox polling.
3. **Only your friends can see what you post** — read access to your content is
   gated, not just "not listed."

These three requirements are a different *posture* than stock dais, but they sit
on top of the same Rust core and the same provider-trait abstraction
(`core/src/traits/*`). This document specifies the additions and the honest
limits.

---

## 2. Threat model (read this first)

Privacy claims must be precise, because federation has a hard ceiling.

| Guarantee | Achievable on open ActivityPub? | How |
| --- | --- | --- |
| Casual strangers can't see your posts | **Yes** | followers-only default + authorized-fetch gate on read endpoints |
| Non-followers can't pull your post by URL | **Yes** | signed-GET (authorized fetch) enforcement |
| Search engines / scrapers don't index you | **Yes** | no public outbox entries; `robots`, no unauthenticated object serving |
| A **follower's** home server admin can't read your followers-only post | **No** | the post is delivered in plaintext to their inbox; their server stores it |
| A follower can't screenshot / re-share | **No** | out of scope for any system |

**Design principle — privacy through user control, not network isolation.**
dais must **not** become a walled garden. It always interoperates with any
ActivityPub or Bluesky server and any other dais instance. Privacy comes from the
*user* deciding each relationship — never from blocking the rest of the network.
We achieve that with three layers, all of which work over **open** federation:

1. **Consent / audience control** — *who is in your graph.* Mutual-follow
   "friends," follower approval, per-post visibility. You choose who you share
   out to and who you consume from. Works with any AP/Bluesky peer.
2. **Authorized-fetch enforcement** — *who can pull.* Only approved followers can
   fetch non-public content; strangers, scrapers, and search engines cannot.
3. **End-to-end encryption (MLS, #71)** — *confidentiality from intermediaries.*
   For content that must stay private even from a recipient's own server, encrypt
   at the content layer. Ciphertext rides open federation through any server;
   only the intended friends hold keys.

**Honest residual risk:** without E2EE, a *followers-only* post is delivered in
plaintext to each follower's inbox, so that follower's home-server operator can
read it. We do **not** solve this by allowlisting "trusted dais servers only" —
that rebuilds the walled garden we're escaping. Instead it's the normal
federation trust model (you trust the servers your friends chose), and **E2EE is
the opt-in answer** when a conversation needs more. Layer 3 makes confidentiality
a property of the *content*, not of a closed *network*.

A user may, of course, block specific peers — but that is user choice on top of a
default-open posture, never a network-level wall.

---

## 3. Core concept changes

### 3.1 "Friends" = mutual follow
ActivityPub follows are one-directional. We derive **friendship** as a mutual
relationship:

```
friend(X)  ≡  followers contains X (status='approved')
             AND following contains X (status='accepted')
```

This needs no new edges — it's a join over the existing `followers` and
`following` tables. We expose it as a SQL view and a `Friendship` concept in core.

### 3.2 Consumption side (the missing half)
Today the inbox (`core/src/activitypub/inbox.rs`) handles Follow / Undo / Like /
Announce / Accept / Reject and only *persists* replies and DMs. It throws away
`Create(Note)` activities that aren't replies to us. Private mode requires
**ingesting** those into a local timeline store so you can read your friends'
posts — including their followers-only posts, which never appear in an outbox.

### 3.3 Inverted defaults + read gating
- Default post visibility flips `public` → `followers`.
- Read endpoints (`actor`, `outbox`, individual object fetch) require a valid
  HTTP signature from an **approved follower** before returning non-public data.

---

## 4. Data model changes

New migration `cli/migrations/009_private_mode.sql`. SQLite/D1 dialect (the only
supported backend after the platform cleanup — see §8).

```sql
-- 4.1 Inbound timeline: posts authored by people we follow, pushed to our inbox.
CREATE TABLE IF NOT EXISTS timeline_posts (
    id              TEXT PRIMARY KEY,         -- our local row id
    object_id       TEXT UNIQUE NOT NULL,     -- remote Note "id" (dedupe key)
    author_actor_id TEXT NOT NULL,            -- remote actor URL
    author_handle   TEXT,                     -- @user@domain (cached for display)
    content_html    TEXT,
    content_text    TEXT,
    in_reply_to     TEXT,
    visibility      TEXT,                      -- as asserted by sender
    media_json      TEXT,                      -- JSON array
    published_at    DATETIME NOT NULL,
    received_at     DATETIME DEFAULT CURRENT_TIMESTAMP,
    raw_json        TEXT                       -- full activity for re-render/debug
);
CREATE INDEX IF NOT EXISTS idx_timeline_published ON timeline_posts(published_at DESC);
CREATE INDEX IF NOT EXISTS idx_timeline_author    ON timeline_posts(author_actor_id);

-- 4.2 Friendship view (mutual follow). Derived, not stored.
CREATE VIEW IF NOT EXISTS friends AS
SELECT f.follower_actor_id AS actor_id
FROM followers f
JOIN following g
  ON g.following_actor_id = f.follower_actor_id
WHERE f.status = 'approved' AND g.status = 'accepted';

-- 4.3 Instance privacy settings (single row, single-user server).
CREATE TABLE IF NOT EXISTS instance_settings (
    id                          INTEGER PRIMARY KEY CHECK (id = 1),
    default_visibility          TEXT NOT NULL DEFAULT 'followers',
    require_authorized_fetch    INTEGER NOT NULL DEFAULT 1,   -- bool
    manually_approves_followers INTEGER NOT NULL DEFAULT 1,   -- bool
    updated_at                  DATETIME DEFAULT CURRENT_TIMESTAMP
);
INSERT OR IGNORE INTO instance_settings (id) VALUES (1);

-- 4.4 No federation allowlist. dais is default-open and interoperates with every
-- AP/Bluesky/dais peer. Optional user-driven peer filtering reuses the existing
-- `blocks` table (migration 003) — block is opt-in on top of open federation,
-- never an allowlist. Confidentiality from intermediary servers is provided by
-- E2EE at the content layer (#71), not by restricting which servers we federate.
```

Note: `posts.visibility` already supports `public|unlisted|followers|direct`
(`001_initial_schema.sql:56`) — no change needed to the posts table, only to the
default applied when creating a post.

---

## 5. Behavioral changes by component

### 5.1 Inbox ingestion — `core/src/activitypub/inbox.rs`
Extend `handle_create` (currently `inbox.rs:200`). Today it returns early for any
`Create(Note)` that isn't a DM or a reply to us. Add a branch:

```
if author is in `following` (status='accepted'):
    upsert into timeline_posts (dedupe on object_id)
```

Also handle `Update(Note)` (edit) and `Delete(Note)` against `timeline_posts` so
the feed stays consistent. Verify the activity's HTTP signature **before**
ingesting (see §5.4) so we don't store spoofed posts.

### 5.2 Authorized fetch — read endpoints
Add a shared `authorize_read(req, db) -> ReadScope` helper in core, where
`ReadScope ∈ {Public, Follower}`. It:
1. Looks for an HTTP `Signature` header on the GET.
2. Resolves `keyId` → actor, verifies the signature (reuse
   `core/src/activitypub/signatures.rs` verify path).
3. Returns `Follower` iff that actor is an approved follower; else `Public`.

Apply it in:
- **actor** (`platforms/cloudflare/workers/actor`): when
  `require_authorized_fetch` and scope is `Public`, return a minimal actor
  (id, public key, inbox, endpoints) and omit profile/follower counts.
- **outbox** (`core/src/activitypub/outbox.rs:71`): already filters to
  `public|unlisted`. With authorized fetch on and scope `Public`, return an
  **empty** OrderedCollection (count only). With scope `Follower`, you may
  include `followers`-visibility posts.
- **object fetch** (individual Note): 404 for `Public` scope when the post is
  `followers`/`direct`.

This is also the fix for the existing "signatures verified but not enforced"
debt — private mode makes enforcement mandatory rather than optional.

### 5.3 Post creation defaults — `core/src/lib.rs:116` (`create_post`)
`create_post(content, visibility)` should treat an unspecified visibility as
`instance_settings.default_visibility` (i.e. `followers`) rather than hardcoding
`public`. The CLI (`dais post create`) drops its implicit public default and
reads the instance setting; `--visibility public` remains an explicit opt-in.

### 5.4 Signature enforcement on inbox
Inbound `POST /inbox` must reject (HTTP 401) activities whose HTTP signature
fails to verify, instead of logging-and-continuing. Required both for security
and so timeline ingestion can't be spoofed.

### 5.5 User-controlled peer filtering (optional, default-open)
dais defaults to open federation and never pre-blocks or pre-approves servers.
Users may *optionally* block specific actors or domains they don't want to share
with or consume from — this extends the existing `blocks` table (migration 003).
There is no default allowlist and no "dais-servers-only" mode. When a
conversation needs confidentiality from intermediary servers, the answer is E2EE
(§5.6 / #71), not network restriction.

---

## 6. Read / timeline API & UI

### 6.1 New core read path
`get_home_timeline(limit, before) -> Vec<TimelinePost>` over `timeline_posts`
ordered by `published_at DESC`. No outbox polling. The existing
`cli/dais_cli/commands/timeline.py` `view` command is **replaced** to read this
local store instead of HTTP-fetching outboxes (delete the per-user outbox GET
loop at `timeline.py:46-86`).

### 6.2 Surfaces
- **CLI/TUI first** (consistent with current product): `dais timeline` reads the
  local store; TUI gets a home-feed pane.
- **Optional authenticated web reader** (later): a single worker route behind
  Cloudflare Access that renders the home timeline for the owner only. This is
  the one place a "rich web UI" is justified despite `VISION.md:66`, because the
  product is now consumption-oriented. Keep it owner-only; do not expose it
  unauthenticated.

---

## 7. Visibility enforcement matrix (target behavior)

| Requester | Public post | Unlisted | Followers-only | Direct |
| --- | --- | --- | --- | --- |
| Anonymous GET (no sig) | served if `require_authorized_fetch=0`, else minimal/empty | hidden | hidden | hidden |
| Signed GET, not a follower | served | served by link | hidden (404) | hidden |
| Signed GET, approved follower | served | served | served | only if addressed |
| Delivered to follower inbox (push) | yes | yes | yes (approved only) | only addressee |

The push side already honors this (`delivery.rs:30` delivers followers-only to
`status='approved'` only). The gap this design closes is the **pull** side.

---

## 8. Platform scope: Cloudflare only

This design assumes the platform cleanup (separate doc/PR) has landed:
- Delete `platforms/vercel/` (27 files) and the legacy `workers/` tree.
- Remove `vercel`/`netlify` Cargo features from `core/Cargo.toml:51-53`.
- Single backend: **D1 (SQLite)** for data, **R2** for media, **CF Queues** for
  delivery, **Cloudflare Access** for owner auth.

Keeping the provider traits (`DatabaseProvider`, etc.) is fine and cheap — they
give clean seams for tests — but only the Cloudflare implementations ship.

---

## 9. Milestones

1. **M1 — Enforce what exists.** Mandatory inbox signature verification (§5.4) +
   authorized-fetch helper (§5.2) wired into actor/outbox. Flip default
   visibility to `followers` (§5.3) + `instance_settings` migration. *No new
   product surface; pure posture flip. Highest security ROI.*
2. **M2 — Consumption.** Timeline ingestion in inbox (§5.1) + `timeline_posts`
   migration + `get_home_timeline` + rewrite `timeline.py` to read locally (§6.1).
3. **M3 — Friends.** `friends` view + mutual-follow helpers; surface "friends"
   vs "followers" in CLI/TUI; optional friends-only visibility tier.
4. **M4 — Web reader (optional).** Owner-only authenticated home-feed route.
5. **M5 — E2EE for DMs (MLS over open federation, #71).** Confidentiality from
   intermediary servers with no walled garden — encryption at the content layer,
   not network restriction. Optional user-driven peer block controls (§5.5) ride
   alongside as ordinary UX.

M1 is independently valuable even if the rest slips — it hardens the server and
makes "only my followers see my posts" actually true on the pull side.

---

## 10. Testing strategy

The repo currently has **no Rust integration tests** — only inline `#[cfg(test)]`
units in core and Python CLI tests. Private mode changes security-critical read
paths, so this gap must close. See the companion testing plan; in summary:

- **Core unit tests** (already the pattern): `authorize_read` scope resolution,
  friendship view logic, default-visibility resolution, timeline upsert/dedupe.
- **Signature tests**: extend `core/src/activitypub/signatures.rs` tests to cover
  verify-and-reject for inbox; valid/invalid/missing/replayed signatures.
- **A `core/tests/` integration suite** using an in-memory `DatabaseProvider`
  fake (the trait makes this straightforward) to exercise: ingest→timeline,
  followers-only post not served to non-follower, served to follower.
- **Federation smoke test**: a scripted local Mastodon (Docker) ↔ dais follow +
  followers-only post + authorized fetch, asserting a non-follower fetch is
  denied. Extends the existing `scripts/test-*.sh` approach.

---

## 11. Open questions

1. **Friends-only as a distinct visibility tier?** ActivityPub has no native
   "mutuals-only" addressing. Options: (a) treat friends-only as followers-only
   on the wire but enforce mutual on our read gate; (b) skip it, keep
   followers-only. Recommendation: (a), behind a flag, in M3.
2. **Backfill on new follow?** When you follow someone, their followers-only
   history won't have been pushed to you. Do we pull their public outbox once for
   initial context, or start the timeline from "now"? Recommendation: start from
   now; optional one-time public backfill.
3. **Default for replies/DMs ingestion vs timeline** — keep DMs in
   `direct_messages` (migration 007) separate from `timeline_posts`. Confirmed.
4. **Web reader auth** — rely solely on Cloudflare Access, or add a dais-native
   session? Recommendation: Access only, to avoid building auth.
