# Design: Dais Desk Product UX Principles

**Status:** Accepted design target for issue #203.
**Scope:** Product-level UX principles and screen model for the first-party Dais
Desk GUI client.
**Related:** `docs/POSITIONING.md`, `docs/design/PRIVATE_MODE.md`,
`docs/design/DAIS_DESK_INFORMATION_ARCHITECTURE.md`,
`docs/design/DAIS_DESK_DESIGN_SYSTEM.md`, `docs/guides/OWNER_TAURI_APP.md`.

## 1. Purpose

Dais Desk is not an ActivityPub admin panel. It is the owner's calm social
workspace for a private-by-default personal network.

The GUI exists so the owner can:

- Read posts from friends, follows, watches, public sources, and direct
  conversations.
- Reply, like, boost/repost, DM, and publish without losing audience context.
- Post publicly, privately to followers/friends, or directly to named people.
- Understand who can see each post before sending it.
- Manage relationships: following, followers, friends, watches, audience groups,
  mutes, and blocks.
- Triage notifications, DMs, replies, delivery failures, and moderation queues.
- Operate the server when attention is needed, without making protocol machinery
  the primary daily experience.

The product promise is:

> I can participate in the social web without accidentally exposing my graph, my
> interests, or my private posts.

## 2. Design Center

Dais Desk is designed first for a privacy-seeking owner/operator, not for
growth, virality, analytics, or platform administration.

The main persona wants:

- A small private social home.
- A clear distinction between public, followers/friends, direct, encrypted, and
  watch-only activity.
- Confidence that sensitive follows, watches, posts, replies, and audience
  groups are not accidentally exposed.
- A good daily reading and response flow.
- Operator controls that are available but not constantly competing for
  attention.

The closest product feeling is a focused mail client plus a feed reader, with a
small operator console attached.

## 3. Core Workflows

The GUI should optimize for these workflows, in this order:

1. **Daily inbox**: read new posts, notifications, DMs, replies, follower
   requests, and attention items.
2. **Respond**: reply to a post, reply from a notification, answer a DM, approve
   or hide a reply, like, boost/repost, or mark read.
3. **Publish**: write a post, choose audience, preview where it can appear,
   attach media with alt text, and send deliberately.
4. **Find and connect**: find someone, follow or watch them, inspect relationship
   state, and avoid confusing Follow with Watch.
5. **Manage people**: approve followers, review following, understand friends,
   maintain audience groups, and block or mute when needed.
6. **Privacy review**: see what is public, followers-only, direct, encrypted,
   watched privately, blocked, muted, or pending.
7. **Operate**: fix failed deliveries, review diagnostics, adjust moderation,
   and manage accounts/tokens.

If a screen does not support one of these workflows, it should either be removed
from primary navigation or moved into an advanced/detail surface.

## 4. What Should Be Easy

The easiest actions should be common and safe:

- Read latest posts.
- Open a thread or notification context.
- Reply.
- Mark a notification read.
- Compose a followers-only post.
- Send a direct message.
- Approve or reject a follower request.
- Follow an account.
- Watch public posts without notifying the remote account.
- Open a person's feed/profile.
- See who can see a draft before posting.

The UI should make these actions visible and close to the relevant content.

## 5. What Should Be Deliberate

These actions should be available but not visually dominant:

- Post publicly.
- Route to Bluesky or other public surfaces.
- Delete a post.
- Remove a follower.
- Block an actor or domain.
- Change account token.
- Change moderation policy.
- Retry or cancel delivery.
- Revoke media.
- Force public search for sensitive-looking queries.
- View raw protocol objects or diagnostics.

They belong behind confirmation, a secondary button, an inspector, or a menu
depending on consequence.

## 6. Visual Posture

Dais Desk should feel like a quiet macOS productivity app:

- Calm, dense, and legible.
- Predictable source-list navigation.
- Stable rows and inspectors.
- Restrained borders and spacing.
- State chips for visibility, relationship, read state, delivery, and
  moderation.
- Clear hierarchy: actor, event, content, context, then actions.
- Text and controls that explain consequences in user language.

It should not feel like:

- A marketing page.
- An engagement dashboard.
- A protocol debugger.
- A wall of raw URLs.
- A general-purpose admin console.
- A card-heavy landing page.

## 7. Screen Space Rules

Screen space should be spent on human meaning:

- Post content.
- Notification context.
- Actor identity.
- Relationship state.
- Audience and visibility.
- Read/unread and attention state.
- The next likely action.
- Safety-critical warnings before the user acts.

Screen space should not be spent on implementation details by default:

- Full actor URLs.
- Inbox or shared inbox URLs.
- Activity IDs.
- Delivery IDs.
- Raw protocol object IDs.
- Full provider error payloads.
- JSON policy blobs.
- Long diagnostic text.

Those details belong in inspectors, details disclosures, copy menus, diagnostics,
or raw/debug views.

## 8. Inline, Hidden, and Menu Information

### Show Inline

Show information inline when it changes a user's immediate decision:

- Who acted.
- What happened.
- What content it concerns.
- Who can see it.
- Whether it is public, followers/friends, direct, encrypted, or watch-only.
- Whether the relationship is friend, follower, following, pending, watch, mute,
  or block.
- Whether action is needed.
- Why a destructive or public action is risky.

### Hide or Collapse

Hide information that is useful only for troubleshooting or copying:

- Raw URLs and IDs.
- Full delivery traces.
- ActivityPub or AT Protocol payloads.
- Exact database, worker, queue, or storage names.
- Complete moderation classifier output.
- Advanced source/provider diagnostics.

### Put in Menus

Menus should hold actions that are valid but not primary:

- Copy link.
- Copy actor URL.
- Open original.
- View raw ActivityPub or AT object.
- Inspect delivery.
- Retry delivery.
- Revoke media.
- Delete post.
- Remove follower.
- Block actor/domain.
- Advanced moderation actions.
- Change protocol route.

Primary inline buttons should stay few and obvious: `Reply`, `Open context`,
`Mark read`, `Approve`, `Reject`, `Follow`, `Watch`, `Send`, and `Post`.

## 9. Privacy and Safety Rules

Privacy is the product. The UI must make audience and graph consequences obvious
before the owner acts.

Rules:

- Every compose surface shows audience before protocol route.
- Every post row/detail shows visibility.
- Every relationship surface distinguishes Follow, Friend, Follower, and Watch.
- Every public-post path uses explicit language like `Post Publicly`.
- Watch never implies a remote relationship or remote approval.
- Followers/friends graph information is owner-only by default.
- Sensitive follows, watches, audience groups, and searches should not be
  presented as public profile material.
- Protocol details can support trust decisions, but they should not be primary
  labels for ordinary social actions.

## 10. Screen and Interface Model

### Global Shell

**Purpose:** The persistent frame for all owner work.

**Major functions:**

- Switch account/instance.
- Navigate Home, People, and Server modes.
- Start compose from anywhere.
- Start global find/search from anywhere.
- Show attention counts for unread notifications, DMs, follow requests,
  moderation queue, and delivery failures.
- Show current default visibility and owner API/token health.

**Visuals:**

- macOS-style source list.
- Compact toolbar.
- Account identity visible but not oversized.
- Attention indicators as small count badges.
- No protocol names in primary navigation.

### Home / Today

**Purpose:** The default daily social surface.

**Major functions:**

- Read Friends, Following, Mentions, DMs, Watches, Saved, Drafts, and My Posts
  lanes.
- Triage what needs action today.
- Open posts, threads, notifications, DMs, and context previews.
- Mark items read/done.
- Reply, like, boost/repost, or open details.

**Visuals:**

- Feed/list layout with a right-side inspector on desktop.
- Lanes or filters for daily queues.
- Rows emphasize author, content, visibility, relationship, and timestamp.
- Attention cards are compact and action-oriented.

### Inbox / Notifications

**Purpose:** Explain what happened and what it is about.

**Major functions:**

- Show mentions, replies, likes/favorites, boosts/reposts, follows, follow
  requests, and other account activity.
- Show notification copy as safe rich text.
- Show a `What this is about` context preview for the referenced post when
  available.
- Open related post/thread with `Open context`.
- Mark notifications read.

**Visuals:**

- Split unread first and read archive.
- Human event title: `Reply from Alice`, `Like from Bob`.
- Message body in a readable block.
- Context preview as a smaller supporting block with visibility/protocol/time
  chips.
- No visible raw activity IDs or long post URLs unless opened in details.

### Thread / Post Inspector

**Purpose:** Let the owner understand and act on a post in context.

**Major functions:**

- Show selected post, thread, replies, likes, boosts/reposts, media, and
  visibility.
- Reply, like, boost/repost, copy link, open original, delete own post, revoke
  media.
- Show delivery and moderation summaries when relevant.

**Visuals:**

- Inspector or pushed detail view.
- Main content first.
- Audience and relationship chips near the title.
- Delivery/moderation details collapsed unless they need attention.

### Compose

**Purpose:** Publish safely and deliberately.

**Major functions:**

- Select identity/account.
- Select audience: Public, Unlisted, Followers/Friends, Direct.
- Select protocol route when needed.
- Choose audience groups and direct recipients.
- Add media and alt text.
- Preview where the post can appear.
- Show sensitive-content and public-sharing warnings.
- Send public posts, followers-only posts, direct posts, or encrypted DMs.

**Visuals:**

- Sheet or focused compose view.
- Audience controls before text/actions.
- Preview panel showing surfaces and recipients.
- Primary action label states consequence: `Post Publicly`, `Post to Followers`,
  `Send Direct`, `Send Encrypted DM`.

### Direct Messages

**Purpose:** Private person-to-person conversations.

**Major functions:**

- Read DM threads.
- Reply or start a new direct conversation.
- Show encryption/federation plaintext status.
- Open the sender relationship card.

**Visuals:**

- Conversation list plus message/thread view.
- Direct/encrypted state is highly visible.
- Raw recipient URLs hidden behind details/copy actions.

### Find / Search / Discovery

**Purpose:** Find posts, people, sources, and public content without making the
  user think in protocol names.

**Major functions:**

- Search local posts and actors.
- Search public providers.
- Resolve handles, actor URLs, Bluesky handles, post URLs, RSS/Atom feeds, and
  domains.
- Follow, Watch, Reply, Like, Boost, or Open when those actions are valid.
- Show sensitive-query guardrails for public search.

**Visuals:**

- One search/finder surface with filters in a compact control area.
- Results grouped by Posts, People, Sources, and Provider Issues.
- Result actions are state-aware and only shown when valid.
- Provider/protocol details are secondary metadata.

### Relationship Card

**Purpose:** Explain how one person/source relates to the owner.

**Major functions:**

- Show whether the owner follows them.
- Show whether they follow the owner.
- Show whether they are a friend/mutual.
- Show whether they are watched, muted, blocked, or pending.
- Follow, unfollow, watch, message, approve, reject, mute, block.

**Visuals:**

- Avatar/source icon, display name, handle/domain.
- Relationship diagram or compact state chips.
- Follow and Watch are visually distinct.
- Trust/provenance evidence shown as supporting detail.

### Friends

**Purpose:** Manage mutual private-sharing relationships.

**Major functions:**

- List mutual relationships.
- Open friend feed/context.
- Message or inspect a friend.
- Identify which friends can see followers/friends content.

**Visuals:**

- Relationship-focused list.
- `Friend` chip prominent.
- Owner-only graph language visible but not alarmist.

### Followers

**Purpose:** Control who can see private/followers content.

**Major functions:**

- Review pending follow requests.
- Approve, reject, remove, or re-approve followers.
- Inspect follower relationship context.
- Link to audience/group membership where relevant.

**Visuals:**

- Pending first, approved second, rejected/removed less prominent.
- Only show actions that make sense for the current status.
- State chips explain access consequence.

### Following

**Purpose:** Manage accounts the owner follows.

**Major functions:**

- List followed accounts and pending states.
- Unfollow active follows.
- Open following feed.
- Add a follow by handle/URL.

**Visuals:**

- Feed plus relationship list.
- Treat following graph as owner-only sensitive information.
- Do not show follow actions for already-followed accounts.

### Watches and Sources

**Purpose:** Privately monitor public content without creating a remote
relationship.

**Major functions:**

- Add watch targets: ActivityPub actor/post, Bluesky actor/post, RSS, Atom.
- Refresh watches/sources.
- Remove watch targets.
- Read harvested public items.

**Visuals:**

- `Watch` state visually distinct from `Follow`.
- Public items read like feed rows.
- Source URL/domain shown compactly, not as long raw strings.
- Explain no remote follow request through concise state labels, not paragraphs.

### Audience Groups

**Purpose:** Create small intentional sharing sets.

**Major functions:**

- Create/edit groups like Close Friends, Family, Work.
- Pick approved followers as members.
- Configure allowed sensitive categories.
- Use groups from Compose.

**Visuals:**

- Group cards with member count and sensitivity category chips.
- Editing form emphasizes members and consequences.
- Empty states point to approved followers.

### Blocks and Mutes

**Purpose:** Reduce unwanted contact and content.

**Major functions:**

- Block/mute actors.
- Block domains.
- Remove blocks/mutes.
- Explain whether a block affects reading, sharing, delivery, or moderation.

**Visuals:**

- Calm safety surface.
- Destructive actions require text labels and confirmation.
- Domain blocks clearly separate from actor blocks.

### Moderation

**Purpose:** Review replies and manage safety policy.

**Major functions:**

- Review queued/flagged replies.
- Approve, hide, reject.
- Manage blocklist and allowlist entries.
- Configure reply policy and AI advisory mode.
- Show deterministic rules as authoritative and AI as advisory.

**Visuals:**

- Queue first when there are pending replies.
- Policy controls separated from individual reply actions.
- AI advisory summaries visible only where relevant.
- Hidden/rejected states are clear and non-ambiguous.

### Deliveries

**Purpose:** Explain where posts went and what failed.

**Major functions:**

- Show failed, queued/retrying, and delivered sends.
- Open target when useful.
- Link back to related post.
- Retry/cancel where supported.

**Visuals:**

- Failed first.
- Status chips: Failed, Queued, Retrying, Delivered.
- Recipient/target labels compacted to domains unless details are opened.
- Delivery IDs hidden unless copied from details.

### Identity / Profile

**Purpose:** Manage public identity.

**Major functions:**

- Edit display name, actor type, summary, avatar, and header.
- Preview public handle and actor URL.
- Make clear what profile fields are public.

**Visuals:**

- Public preview plus form.
- Public fields labeled as public.
- Raw actor URL available but not visually dominant.

### Accounts and Tokens

**Purpose:** Manage local client profiles for multiple Dais instances.

**Major functions:**

- Store account labels, instance URLs, and owner tokens.
- Switch active account.
- Forget non-active accounts.
- Rotate/update tokens.

**Visuals:**

- Local profile cards.
- Active account clearly marked.
- Token state visible without revealing token value.
- Destructive token/account actions in menus or confirmations.

### Settings

**Purpose:** Set defaults and policies that affect future behavior.

**Major functions:**

- Default audience.
- Default protocol route.
- Authorized-fetch/manual approval settings.
- Media policy.
- Sensitive warning policy.

**Visuals:**

- Grouped forms.
- Consequence-focused help text.
- Save buttons disabled until there are valid changes.
- Public defaults highlighted as higher risk.

### Health and Diagnostics

**Purpose:** Verify the instance and troubleshoot problems.

**Major functions:**

- Show owner API, worker, D1/R2/queue, profile, and federation health.
- Provide copyable evidence for debugging.
- Separate passing checks from items needing attention.

**Visuals:**

- Health summary first.
- Needs-attention checks first.
- Raw logs/evidence collapsed.
- Cloudflare/protocol/database terms acceptable here.

### Stats

**Purpose:** Help operate the instance, not optimize engagement.

**Major functions:**

- Counts for posts, followers, following, media, deliveries, notifications,
  moderation, and network mode.
- Link counts to owning screens.

**Visuals:**

- Operational metric cards.
- No engagement leaderboard framing.
- Failed/attention metrics visually prioritized over vanity totals.

## 11. Evaluation Checklist

Each Dais Desk change should be evaluated against these questions:

- Can the owner tell who can see the content?
- Can the owner tell whether an account is followed, follower, friend, watch,
  muted, blocked, or pending?
- Are common safe actions inline and rare/destructive actions secondary?
- Are raw URLs and protocol IDs hidden unless needed?
- Does the screen show content/context before implementation details?
- Does a notification explain what happened and what it is about?
- Does compose make public sharing unmistakable?
- Does the screen stay usable with long names, URLs, counters, and empty states?
- Is protocol detail used only where it helps trust, delivery, or debugging?
- Would this still feel calm after checking it every day?
