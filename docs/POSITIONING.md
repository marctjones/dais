# dais — Positioning

**Status:** Source of truth for *why dais exists and who it's for.*
**Supersedes:** the creator/influencer framing in `docs/archive/STRATEGY_NON_TECHNICAL_USERS.md`,
the public-broadcast emphasis in older `docs/archive/VISION.md`, and any "100% complete" /
"encrypted DMs ✅" claims in `README.md` that don't match §5.

This document states, once and coherently: **purpose → persona → protocols →
privacy model → business model**, plus an honest status snapshot. Companion
design doc: `docs/design/PRIVATE_MODE.md`. Product research on mainstream social
platform lessons lives in `docs/research/MAINSTREAM_SOCIAL_PLATFORM_LESSONS.md`.
Work tracked under epic #70.

---

## 1. Purpose

**dais lets you own your social life instead of renting it from a platform —
private by default.**

### The whole product, in plain terms

A person on dais can do three things:

1. **Post publicly** — broadcast to the world, like Threads / Bluesky / Twitter.
2. **Post privately to your friends** — friends-only, like Facebook at launch.
3. **DM a specific person** — a direct, end-to-end-encrypted message.

**DMs complement** the two posting modes — the private side-channel, not a
separate product. Everything else in this document is detail on top of these
three actions. Under the hood: public post → Bluesky + public ActivityPub;
private post → ActivityPub followers-only (your friends graph); DM → MLS-encrypted
direct (#71).

### The feeling

The feeling to recreate is *Facebook at launch*: you see posts from your
friends, and the people who see your posts are your actual friends. No algorithm,
no surveillance feed, no platform that can ban you or sell your graph. But unlike
2004 Facebook, dais is **federated and self-sovereign** — your identity lives on
your domain, and you interoperate with the wider social web on your terms.

dais is a **personal** social server, not a publishing appliance and not a
creator-broadcast tool. Those are *capabilities it also has* (§3), not the
mission.

One-line: **a self-sovereign, private-by-default personal social network that
interoperates with the whole fediverse and Bluesky — where the user decides every
connection.**

---

## 2. Persona

### Design center — privacy-seeking individuals and small groups
The **core capabilities are built for** people who want a small, private social
space and are uneasy with surveillance feeds: friends who want a shared space,
families, close-knit communities, and privacy-conscious individuals fleeing
ad-driven platforms. They value *control and calm*, not reach. Every default and
every primitive is designed for them first.

This is dais's **defensible wedge**: nobody else offers *truly private +
self-sovereign + federated*. Mastodon can't (public-leaning, instance-owned),
Bluesky won't (public by design), and the incumbents are the thing these users
are leaving.

### Complementary — content creators with public platforms
A privacy-first network is **not** inconsistent with public creators — they
strengthen it. dais already separates the **private friends graph** (ActivityPub,
followers-only) from the **public broadcast surface** (Bluesky + public AP, §3),
so a creator simply operates more on the public surface while a privacy-seeker
operates more on the private one. *Same network, same primitives, different
emphasis.*

The synthesis: a privacy-seeking user can **follow and consume a creator's public
content into their private home timeline without exposing themselves.** The
creator gets reach; the consumer keeps their privacy. Creators producing content
on the network is a **flywheel** — it gives the private community something to
consume and gives the network gravity — not a compromise of the mission. Creators
are welcome from the start.

**The one caution is go-to-market, not architecture:** we design for the privacy
persona *first*, and we don't *lead* acquisition with the highest-liability
creator segments (e.g. adult content → moderation/DMCA/GDPR burden). That's
sequencing of outreach, not an exclusion — creators are part of a healthy
ecosystem, hosted on the same rails.

### Also supported — families, communities, and small businesses

The same single-user server can represent more than one social posture:

- **Personal presence**: `@yourname@yourdomain.com`, matching the pattern people
  already understand from email and personal homepages.
- **Small-group/community presence**: a managed `Group` actor for a family,
  close community, club, or project where one operator maintains the server.
- **Business presence**: `@social@businessdomain.com`, a managed `Organization`
  actor that gives a business a Fediverse identity without replacing its main
  website.

These are not a move toward a general multi-tenant platform. They are
deployment modes for the same owner-operated dais instance: one operator, clear
posting identity, private-by-default visibility rules, and Mastodon-safe
fallbacks when richer ActivityStreams semantics are not understood by a remote
server.

---

## 3. Protocols

Protocols are substrate; purpose decides which matter and in what role. dais is
**Cloudflare-only** (D1 / R2 / Queues / Access); the provider-trait abstraction
stays for testability, but only Cloudflare ships. (Vercel/Netlify dropped — #57.)

| Protocol | Role | Status |
|---|---|---|
| **ActivityPub** | **Primary.** The private/friends surface *and* public federation. Mastodon/Pleroma/Pixelfed interop. | Mature core; private-mode additions pending |
| **AT Protocol / Bluesky** | **Public broadcast surface.** Your public voice to the open world. First-class for *public* content only. | ~50%, experimental |
| **WebFinger + HTTP Signatures** | Required substrate (discovery + authenticity). | Mature; inbound enforcement pending (#60) |
| **MLS (RFC 9420)** | **E2EE for DMs / private content** over open federation. | Spike (#71) |

**Mastodon is not a protocol** — it's an ActivityPub implementation we
*interoperate with*. We do not reimplement Mastodon.

**Network roles, stated once:**
- **ActivityPub = your private/friends graph** (followers-only default,
  mutual-follow friends, authorized-fetch).
- **Bluesky = your public face** (public posts + identity + reading). Private
  content is **never** sent to Bluesky — it can't express it (decision #69).

---

## 4. Privacy model — control, not isolation

**dais is never a walled garden.** It always interoperates with any ActivityPub /
Bluesky / dais peer. Privacy comes from the **user gatekeeping each
relationship**, not from the network blocking everyone else. The goal is to
*maximize interoperability with protocols that have real audiences, while the user
decides every connection.*

Three layers, all over **open** federation:

1. **Consent / audience control** — *who's in your graph.* Mutual-follow
   "friends," follower approval, per-post visibility. You choose who you share out
   to and who you consume from. Works with any peer.
2. **Authorized-fetch enforcement** — *who can pull.* Only approved followers can
   fetch non-public content; strangers, scrapers, and search engines cannot.
3. **End-to-end encryption (MLS, #71)** — *confidentiality from intermediaries.*
   Encrypt at the content layer; ciphertext rides any server, only friends hold
   keys. Privacy becomes a property of the **content**, not the **network**.

**Honest residual risk:** without E2EE, a followers-only post is delivered in
plaintext to each follower's inbox — so that follower's home-server operator can
read it. We do **not** fix this with a "trusted-dais-servers-only" allowlist
(that's the walled garden we're escaping). It's the normal federation trust model
(you trust the servers your friends chose), and **E2EE is the opt-in answer** when
a conversation needs more. Users may also block specific peers — opt-in, on top of
default-open federation.

---

## 5. Implementation status (honest snapshot)

| Capability | Status |
|---|---|
| ActivityPub core (follow/inbox/outbox/webfinger/delivery), outbound signing | **Mature / production** (Cloudflare) |
| CLI / TUI management | **Mature** |
| Inbound HTTP-signature enforcement | **Implemented for inbox POSTs** |
| Authorized-fetch read gating | **Implemented for non-public and encrypted post pulls** |
| ATProto / Bluesky / PDS | **Public-read compatibility floor; AppView work remains** |
| Bluesky **reading** (AppView) | **Personal AppView floor (v0.16) plus a poll-based aggregated follows timeline (v1.36 Track A) and a firehose-based near-real-time consumer (v1.36 Track B, #50): a Durable Object holds a persistent connection to Bluesky's relay, decodes commits for followed DIDs, and indexes posts/likes/follows into D1, merged chronologically with the ActivityPub home timeline in Desk/TUI/CLI. Owner-authenticated `notifications`/`likes`/`followers` endpoints are also live.** |
| Private mode (home timeline, default-private, friends) | **Implemented foundation; still hardening UX and lifecycle coverage** |
| **E2EE DMs** | **Implemented on the MLS/RFC 9420 v2 owner workflow; legacy encryptedMessage v1 rows/devices are purged, and live dais.social <-> skpt.cl gates cover 1:1, groups, multi-device, removal, and delivery-worker processing** |
| Rich ActivityPub objects | **v0.17 foundation: Article, Document, Event, Group, Organization with Mastodon-safe fallbacks** |
| Media / R2 | **Implemented for public/private ActivityPub media, ATProto public image upload, and encrypted media attachments; shared R2 binding abstraction intentionally not exposed yet** |
| Managed hosting (dais.cloud) | **Designed / not launched**. Tier, privacy, family/org, and support boundaries are defined in `docs/guides/MANAGED_HOSTING.md`; provisioning/restore/import/observability workflows remain v1.34 implementation work. |
| Rust integration tests | **Core and client suites exist; broader live federation remains scripted/manual** |

**Reality:** the *public single-user ActivityPub publisher* is largely built. The
*private personal network* this document describes is mostly green-field on top of
a solid AP foundation. Doc/README cleanup tracked in #58/#59.

---

## 6. Business model / adoption

Two of this session's decisions reshape the earlier adoption plan:
- **Cloudflare-only (#57)** removes the "Deploy to Vercel" button and marketplace
  templates — the old semi-technical on-ramp.
- **Private-by-default** removes the creator reach/discoverability value prop the
  old revenue projections were built on.

Coherent path forward:

1. **Now — technical OSS.** Self-hosted on Cloudflare for technically capable
   privacy-seekers. Build the private-mode wedge (#62–#64), harden security
   (#60/#61), establish trust via honesty and tests (#59/#67).
2. **Next — managed dais.cloud** (multi-tenant Cloudflare). The non-technical
   on-ramp for the *privacy* persona — friend groups, families, small communities
   who want it to "just work." Subscription for managed private-network hosting,
   *not* creator monetization. Survives the Cloudflare-only decision intact.
3. **Alongside — creators as a content flywheel.** The public surface works from
   day one (same primitives as the private graph), so creators can produce public
   content on dais without waiting for a separate product. Their presence gives
   the privacy-first community something to consume and pulls audiences in.
   Creator *tooling* and monetization are an expansion built on the same rails —
   cultivated as the base grows, not gated behind it. We just don't *lead*
   acquisition with the highest-liability segments (§2).

**Differentiation in one line:** dais's moat is **privacy + self-sovereignty +
interoperability**, not reach. Reach is a feature; control is the product — and a
private network that *also* hosts public creators is more valuable than either
alone.

---

## 7. What this supersedes / next cleanups

- Rewrite or retire `docs/archive/STRATEGY_NON_TECHNICAL_USERS.md` (creator framing) — #59.
- Update `docs/archive/VISION.md` so purpose and posture are stated once (this doc is the
  reference) — #59.
- Fix `README.md` overclaims to match §5 (esp. "encrypted messaging") — #58/#59.
- Keep this doc and `docs/design/PRIVATE_MODE.md` in sync with epic #70.
