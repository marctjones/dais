# Design: Dais Desk Information Architecture

**Status:** Accepted design target for issue #172.
**Scope:** The first-party GUI client, currently `apps/dais-desk`, as the
owner/operator surface for one or more Dais instances.
**Related:** `docs/POSITIONING.md`, `docs/design/PRIVATE_MODE.md`,
`docs/design/DAIS_DESK_PRODUCT_UX.md`,
`docs/design/DAIS_DESK_DESIGN_SYSTEM.md`, `docs/design/CLIENT_REDESIGN.md`,
`docs/guides/DAIS_DESK_APP.md`.

## 1. Goal

Dais Desk should feel like a private-by-default social home with an operator
console attached. The main navigation should not be a list of implementation
features or protocol nouns. It should answer three user questions:

1. What should I read, reply to, or post now?
2. Who am I connected to, watching, sharing with, or blocking?
3. Is my server healthy, and where did my posts go?

Those questions become the three primary modes:

- **Home**: daily reading, replying, composing, notifications, DMs, watches,
  drafts, saved posts, and the user's own posts.
- **People**: friends, followers, following, follow requests, watched public
  sources, audience groups, blocks, mutes, and discovery.
- **Server**: diagnostics, deliveries, federation health, moderation, public
  identity, account profiles, tokens, settings, statistics, and release/operator
  tasks.

Protocol details still matter, but they belong in inspectors and advanced
details. Primary navigation should use user-language labels.

## 2. IA Principles

- **Private by default is visible everywhere.** Rows, compose sheets, post
  inspectors, and relationship cards show audience state before protocol state.
- **Daily social work and operator work are separated.** Home should not feel
  like a dashboard. Server should not compete with the feed unless attention is
  required.
- **Every capability has one primary home.** A workflow may surface shortcuts in
  other modes, but ownership stays singular so the app remains learnable.
- **Relationship words describe consequences.** Friend, Follower, Following,
  Watch, Muted, Blocked, Pending, and Unknown should be the visible vocabulary.
- **Protocol names are secondary.** ActivityPub, AT Protocol, Bluesky, WebFinger,
  D1, R2, and Workers appear in diagnostics, delivery records, and advanced
  identity views, not in the main sidebar.
- **Risky state changes preview consequences.** Public posting, changing an
  audience, approving a follower, deleting a post, revoking media, blocking, and
  token changes require a plain-language confirmation.

## 3. Global Shell

The shell is shared by all modes:

- **Account switcher**: the current Dais instance and posting identity. Switching
  accounts changes the owner API target for all modes.
- **Mode switcher**: Home, People, Server. These are the only top-level
  navigation groups.
- **Command/search field**: accepts text, handles, URLs, post URLs, feed URLs,
  domains, and commands. Results route to the correct mode.
- **Compose button**: always available, opens a sheet over the current context.
- **Attention indicator**: summarizes unread replies, DMs, follow requests,
  moderation queue, and delivery failures. Selecting an item opens the owning
  mode and screen.
- **Privacy status**: shows the active account's default audience and whether
  the current screen can change public state.

The shell should never require the user to know which protocol a person, post, or
source uses before starting a task.

## 4. Screen Map

### 4.1 Home

Home is the daily social surface. It opens to **Today**.

Primary screens:

- **Today**: combined reading queue with lanes for Friends, Following, Mentions,
  DMs, Watches, Saved, Drafts, and My Posts.
- **Post and Thread Inspector**: selected post, thread, replies, audience,
  relationship, moderation state, and available actions.
- **Compose Sheet**: identity, audience, text, media, sensitive-content warning,
  alt text, recipients, and visibility preview.
- **Inbox Queue**: replies, mentions, DMs, follow requests needing attention,
  moderation-needed replies, and delivery failures grouped by urgency.
- **My Posts**: sent posts and drafts, with delete, edit where supported,
  media-revoke, delivery-status shortcut, and visibility context.
- **Saved and Drafts**: owner-only saved posts, drafts, and staged replies.

Home may show Watch content, moderation flags, and delivery failures, but the
configuration surfaces for watches, moderation, and deliveries live in People or
Server.

### 4.2 People

People is the relationship and discovery surface. It answers who sees whom, who
knows about whom, and what is private.

Primary screens:

- **Find**: universal discovery for handles, actor URLs, Bluesky handles, post
  URLs, RSS/Atom feeds, domains, and public search results.
- **Relationship Card**: one person/account/source, showing whether the owner
  follows, is followed by, is friends with, watches, mutes, blocks, or has a
  pending request.
- **Friends**: mutual private-sharing relationships.
- **Followers**: approved, pending, rejected, and removed follower records.
- **Following**: remote accounts the owner follows and whether the relationship
  is accepted, pending, or failed.
- **Watches and Sources**: public-only private monitoring of accounts, feeds,
  and searches that does not notify the remote account.
- **Audience Groups**: Close Friends, Family, Work, and custom groups used by
  compose.
- **Blocks and Mutes**: actor, domain, and content filtering relationships.
- **Bundles**: starter-pack-style sets of follows, watches, and feed presets.

People should make Follow, Friend, and Watch feel different:

- **Follow** may send a remote relationship request.
- **Friend** means mutual private sharing.
- **Watch** reads public posts privately without creating a remote relationship.

### 4.3 Server

Server is the operator console. It should be calm, factual, and separate from
daily reading.

Primary screens:

- **Health**: version, deployment, worker health, database/storage status,
  queue status, public profile reachability, and account-token state.
- **Deliveries**: where each post went, retry state, signing state, recipient
  count, protocol writes, failures, and safe retry/cancel actions.
- **Moderation**: reply queue, blocks, mutes, domain blocks, AI advisories,
  sensitivity policy, and stackable rule layers.
- **Identity**: public actor/profile settings, display name, avatar, header,
  domain identity, handles, and public profile previews.
- **Accounts and Tokens**: local Dais account profiles, instance URLs, owner
  tokens, active account, and token rotation.
- **Settings**: default audience, default posting route, authorized fetch,
  manual follower approval, media policy, and privacy defaults.
- **Diagnostics**: raw checks, logs, copyable evidence, and troubleshooting
  details.
- **Stats**: counts and trends that help operate the server, not engagement
  ranking.

Server is the only mode where Cloudflare, protocol, and database implementation
terms should be prominent.

## 5. Current Capability Routing

This table assigns every current Dais Desk section one primary home in the new
architecture.

| Current section | New primary home | Notes |
| --- | --- | --- |
| Home | Home -> Today | First screen; social reading and attention summary. |
| Compose | Home -> Compose Sheet | Global action, not a persistent sidebar page. |
| Search | People -> Find | Public post search and saved search creation start here. |
| Discovery | People -> Find | Handles, URLs, feeds, domains, posts, and accounts. |
| Notifications | Home -> Inbox Queue | Mentions, replies, DMs, requests, and failures by urgency. |
| DMs | Home -> Inbox Queue | Direct conversations are part of daily social work. |
| Following | People -> Following | Outbound relationships and pending states. |
| Friends | People -> Friends | Mutual private-sharing relationships. |
| Followers | People -> Followers | Includes pending approval work. |
| Audience | People -> Audience Groups | Small sharing sets and sensitivity compatibility. |
| Posts | Home -> My Posts | Delivery and federation diagnostics link into Server. |
| Sources | People -> Watches and Sources | Configuration lives here; read items appear in Home. |
| Watches | People -> Watches and Sources | Private public-post monitoring. |
| Moderation | Server -> Moderation | Flagged items also surface in Home's queue. |
| Deliveries | Server -> Deliveries | Post inspector links here for a selected post. |
| Stats | Server -> Stats | Operational stats, not engagement ranking. |
| Profile | Server -> Identity | Public profile and federation identity. |
| Settings | Server -> Settings | Privacy defaults and instance configuration. |
| Diagnostics | Server -> Diagnostics | Raw operator troubleshooting. |

## 6. Task Map

| User task | Start here | Primary screen | Escape hatch |
| --- | --- | --- | --- |
| Read what matters today | Home | Today | Saved feed preset. |
| Reply to a friend | Home | Post and Thread Inspector | Compose Sheet. |
| Publish a post | Global compose button | Compose Sheet | Server -> Settings for defaults. |
| Send a direct message | Home | Inbox Queue or Compose Sheet | People -> Relationship Card. |
| Find someone to follow | People | Find | Paste into global command/search field. |
| Watch public posts privately | People | Find, then Watches and Sources | Home -> Watches lane. |
| Approve a follower | People | Followers | Home attention indicator. |
| Create a close-friends group | People | Audience Groups | Compose Sheet audience selector. |
| See who can read a post | Home | Post and Thread Inspector | Server -> Deliveries for raw delivery state. |
| Delete or revoke a media item | Home | My Posts | Server -> Deliveries if remote delivery matters. |
| Moderate a reply | Home | Inbox Queue | Server -> Moderation for policy changes. |
| Block an account or domain | People | Blocks and Mutes | Server -> Moderation for policy context. |
| Check whether federation is healthy | Server | Health | Diagnostics for raw evidence. |
| Debug a failed delivery | Server | Deliveries | Open from the post inspector. |
| Edit public profile | Server | Identity | Preview public surfaces before saving. |
| Add another Dais instance | Server | Accounts and Tokens | Account switcher after setup. |
| Rotate an owner token | Server | Accounts and Tokens | Diagnostics if validation fails. |

## 7. Navigation Labels

Use these primary labels:

- Home
- People
- Server

Recommended secondary labels:

- Today
- Inbox
- My Posts
- Saved
- Drafts
- Find
- Friends
- Followers
- Following
- Watches
- Audience Groups
- Blocks and Mutes
- Health
- Deliveries
- Moderation
- Identity
- Accounts and Tokens
- Settings
- Diagnostics
- Stats

Avoid these in primary navigation:

- ActivityPub
- AT Protocol
- Bluesky
- Federation
- D1
- R2
- Workers
- Outbox
- Inbox as a protocol term
- WebFinger

Those terms are acceptable in Server details, delivery inspectors, diagnostics,
developer documentation, and copyable evidence.

## 8. Implementation Phasing

1. **Navigation reshape**: collapse the current Today, People, Library, and
   Operate groups into Home, People, and Server without removing workflows.
2. **Home consolidation**: merge Home, Notifications, DMs, Posts, and read-side
   Watch output into Today and Inbox Queue.
3. **People consolidation**: merge Search, Discovery, Following, Friends,
   Followers, Audience, Sources, Watches, Blocks, and Mutes into Find and
   relationship cards.
4. **Server consolidation**: merge Moderation, Deliveries, Stats, Profile,
   Settings, Diagnostics, account profiles, and tokens into a dedicated operator
   mode.
5. **Protocol demotion**: keep protocol names in inspectors and diagnostics, but
   remove them from primary navigation and ordinary action labels.

The first implementation milestone should keep old routes available as internal
anchors or redirects so existing smoke tests can move gradually.

## 9. Acceptance Checklist

- Every current owner capability has exactly one primary home in the routing
  table.
- The default app opening answers what to read, reply to, or post now.
- People/relationship screens explain visibility and notification consequences
  before changing a relationship.
- Server/operator screens are separated from the daily social feed.
- Public posting, follower approval, media revocation, delete, block, and token
  changes preview consequences before saving.
- Main navigation labels use user-language terms instead of protocol or storage
  terms.
- The global account switcher makes it clear which Dais instance and identity
  the user is operating.
