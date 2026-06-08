# Design: Modular Protocol Adapters + Capability Matrix

**Status:** Confirmed by research pass (June 2026) — see §2. Proposal for build.
**Scope:** Keep dais from hard-coding to ActivityPub. A canonical, protocol-
agnostic core social model + per-protocol **adapters** + a **capability matrix**
that honestly routes each of the three user actions (public post / private
friends post / DM) to the networks that can actually express it.

Companion: `docs/POSITIONING.md` (three-mode product), `docs/design/PRIVATE_MODE.md`,
E2EE DM spike #71. Tracked under epic #70.

---

## 1. Why

dais's product is three actions: **post publicly**, **post privately to friends**,
**DM a person** (`POSITIONING.md`). Each maps to different network capabilities,
and those capabilities differ sharply by protocol. We must not assume every
network can express every action — and we must never leak audience-scoped content
to a network that can't enforce the boundary. A protocol-adapter layer with an
explicit capability matrix is how we stay modular *and* honest.

The seam already exists in embryo: `cli/dais_cli/protocols/manager.py` +
`protocols/atproto.py`, and the core split between `core/src/activitypub/` and
`core/src/atproto/`. This design formalizes it into a trait + capability model.

---

## 2. Research verdict (June 2026, adversarially verified)

A deep research pass (18 sources, 25 claims verified) **confirmed** the
architecture. Key findings and their consequences:

- **ActivityPub is nearly unique in native followers-only posting** (recipient
  addressing: follower collection in `to`/`cc` without `as:Public`). Confirmed
  3-0. [W3C ActivityPub; Mastodon ActivityPub spec]
- **Bluesky/AT Protocol, Nostr, Farcaster, Lens are public-by-design** with no
  native followers-only post visibility. Confirmed 3-0. Bluesky's private-data
  work is "at least a year off" (Oct 2025 roadmap) — Auth Scopes prerequisite
  only finished Oct 2025, design not started. **Not shipped as of June 2026.**
  [docs.bsky.app roadmap; atproto GitHub #1409]
- **Honest-routing precedent — Bridgy Fed.** It bridges ActivityPub ↔ Bluesky ↔
  IndieWeb and **explicitly refuses to bridge** "unlisted, quiet public,
  followers-only, or otherwise private posts or DMs" — *only fully public content
  crosses.* This is the real-world validation of the capability-matrix principle:
  **don't bridge what the target protocol can't express.** Confirmed 3-0.
  [github.com/snarfed/bridgy-fed; fed.brid.gy/docs]
- **AP followers-only enforcement is implementation-dependent and has leaked in
  practice** (Pixelfed disclosed a followers-only leak in 2025). *Consequence:*
  we do not treat AP followers-only as confidentiality — true privacy is E2EE
  (#71), exactly as `PRIVATE_MODE.md` §2 already states.
- **DM/E2EE landscape (refines #71):**
  - **MLS (RFC 9420) + OpenMLS** is the real standard and remains the most
    self-sovereign, embeddable, transport-agnostic option (fits our Rust core).
  - **Matrix** has production E2EE (Olm/Megolm) but **no shipped MLS** (only draft
    MSCs 4244/4256), is a *room/messaging network* not a social-post layer, and
    has **no mature ActivityPub/Bluesky/Nostr bridges**. Refuted: that Matrix
    bridges social protocols or expresses followers-only post semantics (0-3).
    → weaker "DM backbone" candidate than assumed.
  - **Nostr NIP-17** (sealed kind:13 + gift-wrapped kind:1059) is a live, simple
    E2EE DM scheme. Confirmed 3-0. Caveat: metadata privacy is partial (relay
    timing/IP correlation). Nostr-MLS ("Marmot"/NIP-EE) is **experimental, not
    standardized** — the claim it merged Aug 2025 was refuted (0-3).
  - **XMTP dropped:** claims of RFC 9420 MLS, NCC audit, and a decentralized
    testnet **could not be verified and were refuted** (0-3). Centralized.
- **Diaspora dropped:** it *does* support followers-only ("semi-public"), but the
  network is negligible (~15–50K, dev inactive since 2019). Capability without
  audience. Skip.
- **Public reach** (orders of magnitude, mid-2026 estimates): ActivityPub largest
  (Mastodon ~2.5M + Threads ~100M+, bidirectional federation), Bluesky ~9M, Nostr
  ~50K–500K, Farcaster ~300K, Lens tiny.

**Net:** architecture **CONFIRMED**. Three refinements: (a) the capability matrix
must enforce honest routing (Bridgy Fed proves it), (b) drop XMTP and Diaspora,
(c) the DM backbone leans back toward **MLS/OpenMLS over our own transport** —
Matrix and Nostr-MLS are less ready than hoped (resolve in #71).

---

## 3. Capabilities (the matrix columns)

Each adapter declares a `CapabilitySet`:

| Capability | Meaning |
| --- | --- |
| `public_broadcast` | Can post to the open world |
| `private_audience` | Native followers-only / audience-scoped posts |
| `direct_message` | 1:1 (or small-group) direct messages |
| `e2ee_dm` | DMs are end-to-end encrypted |
| `media` | Image/video/file attachments |
| `threading` | Replies / conversation threads |
| `reactions` | Likes / boosts / reposts |
| `edit` / `delete` | Post mutation |

Capability matrix (current, June 2026):

| Protocol | public | private_aud. | DM | e2ee_dm | notes |
| --- | :-: | :-: | :-: | :-: | --- |
| **ActivityPub** | ✅ | ✅* | ✅ | ❌ | *enforcement implementation-dependent; not confidentiality |
| **AT Proto / Bluesky** | ✅ | ❌ | ⚠️ | ❌ | DMs centralized on AppView |
| **Nostr** | ✅ | ❌ | ✅ | ✅ (NIP-17) | metadata privacy partial |
| **Matrix** | ❌ | ❌ | ✅ | ✅ (Olm/Megolm) | rooms, not social posts; no MLS yet |
| **MLS/OpenMLS** | — | — | ✅ | ✅ | a *crypto layer*, not a network (rides our transport) |
| Farcaster / Lens | ✅ | ❌ | ❌ | ❌ | Tier 3, niche |
| ~~XMTP~~ / ~~Diaspora~~ | — | — | — | — | dropped (see §2) |

---

## 4. The canonical core model (protocol-agnostic)

The core speaks *intent*, never wire format:

- **`Identity`** — a dais user's set of per-network identities (AP actor, Bluesky
  DID, Nostr npub …).
- **`Post`** — content, media, and **`audience: Public | Friends | Direct(recipients)`**
  (the three modes, first-class).
- **`SocialGraph`** — follows / followers / friends, normalized across networks.
- **`TimelineItem`** — ingested inbound content, normalized.
- **`Message`** — a DM (may carry an E2EE payload).

---

## 5. The `ProtocolAdapter` trait

```rust
#[async_trait]
trait ProtocolAdapter {
    fn id(&self) -> ProtocolId;                 // activitypub | atproto | nostr | …
    fn capabilities(&self) -> CapabilitySet;

    async fn publish(&self, post: &Post) -> Result<PublishReceipt>;
    async fn withdraw(&self, receipt: &PublishReceipt) -> Result<()>;

    async fn fetch_timeline(&self, cursor: Cursor) -> Result<Vec<TimelineItem>>;
    async fn follow(&self, who: &Identity) -> Result<()>;
    async fn accept_follow(&self, who: &Identity) -> Result<()>;

    async fn send_dm(&self, msg: &Message) -> Result<()>;     // iff direct_message
    async fn resolve_identity(&self, handle: &str) -> Result<Identity>;
}
```

ActivityPub and ATProto become the first two implementations (logic largely
exists in `core/src/activitypub` + `core/src/atproto`). Building the trait against
*two genuinely different* protocols validates it; resist generalizing for Tier 3
hypotheticals.

---

## 6. Honest routing policy (the linchpin)

A capability-aware router maps a user's intent to the adapters that can honor it:

```
route(post):
  targets = adapters where post.audience is expressible by adapter.capabilities()
  for Public   → every adapter with public_broadcast the user has enabled
  for Friends  → only adapters with private_audience  (today: ActivityPub only)
  for Direct   → only adapters with direct_message (+ prefer e2ee_dm)
  NEVER route a Friends/Direct post to a public-only adapter.
```

This is the **principled generalization of the existing Bluesky privacy-downgrade**
in `PRIVACY_GUIDE.md`, and it matches Bridgy Fed's real-world rule exactly
(§2): *don't bridge what the target can't express.* When the user asks for reach
that their audience setting forbids on a given network, the router **drops that
network and tells the user** (never silently downgrades the audience).

**Confidentiality is content-layer, not routing-layer.** Because AP followers-only
enforcement is unreliable across remote servers (§2), "Friends" routing gives
*audience control*, not secrecy. When secrecy is required, the payload is E2EE
(#71) and rides any transport — privacy is a property of the content, per
`PRIVATE_MODE.md` §2.

---

## 7. Tiering (confirmed)

- **Tier 1 — build now:** ActivityPub (public + private friends), Bluesky (public).
- **Tier 2 — design the seam now, build when earned:**
  - **Nostr** — next public network (censorship-resistance, simple keypair
    identity, NIP-17 DMs). Small audience, so value is reach-diversity + ethos,
    not scale.
  - **DM backbone (resolve in #71):** lead candidate **MLS/OpenMLS over dais
    transport** (most self-sovereign, embeddable, Rust-native); alternatives
    Matrix (mature E2EE but heavy, no MLS, immature social bridges) and Nostr
    NIP-17 (simple, live, metadata caveats). DM is a *pluggable transport*, not
    hardwired.
- **Tier 3 — watch only:** Farcaster, Lens (public, niche). **Dropped:** XMTP
  (unverifiable claims), Diaspora (no audience).

---

## 8. Open questions / time-sensitivity

1. **Bluesky private data** — roadmap "at least a year off" (Oct 2025). If it
   ships followers-only posts (late 2026/2027), the AT Proto adapter's
   `private_audience` flips to ✅ and "Friends" routing can include Bluesky.
   *Adapter design must make capability a runtime/feature value, not a constant.*
2. **Matrix MLS** — if MSC4244/4256 ship, Matrix strengthens as a DM backbone.
   Re-evaluate in #71.
3. **DM backbone choice** — MLS-over-transport vs. Matrix vs. NIP-17. Decided in
   the #71 spike, informed by §2.
4. **AP followers-only enforcement** — test against target servers; never assume
   remote enforcement (Pixelfed 2025 leak). Drives #61 (authorized fetch) on our
   own read path and E2EE for real secrecy.

---

## 9. Build seam

- Extract `ProtocolAdapter` + `CapabilitySet` in `core/` (new `core/src/protocol/`).
- Refactor `core/src/activitypub` and `core/src/atproto` to implement it.
- Replace ad-hoc dispatch in `cli/dais_cli/protocols/manager.py` with the
  capability-aware router (§6).
- Two implementations only for now (AP + ATProto). Nostr is the first *new*
  adapter and the real test of modularity — but only when it earns a slot.
