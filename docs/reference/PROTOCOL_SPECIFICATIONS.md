# Protocol Specifications Reference

This document indexes the protocol specifications and implementation-specific
references that matter for dais. Local HTML snapshots are stored under
`docs/reference/specifications/` for offline review. Canonical upstream URLs
remain the source of truth.

Snapshot date: 2026-06-11.

## Local Snapshot Policy

- Treat local files as review snapshots, not authoritative living specs.
- Re-fetch local copies before a release, conformance audit, or federation
  compatibility change.
- Keep conformance tests split by source:
  - `SPEC`: W3C/IETF/official protocol requirements.
  - `MASTODON`: Mastodon-published conventions and extensions.
  - `DAIS-PRIVACY`: stricter private-by-default behavior.
  - `ATPROTO` or `BLUESKY`: AT Protocol and Bluesky application-specific behavior.

## ActivityPub, Mastodon, and WebFinger

| Area | Canonical source | Local copy | Applies to dais |
| --- | --- | --- | --- |
| ActivityPub | https://www.w3.org/TR/activitypub/ | `specifications/activitypub/w3c-activitypub.html` | Actor, inbox, outbox, object dereferencing, addressing, delivery, authorization model |
| ActivityStreams 2.0 Core | https://www.w3.org/TR/activitystreams-core/ | `specifications/activitypub/w3c-activitystreams-core.html` | JSON object model, activities, collections |
| ActivityStreams 2.0 Vocabulary | https://www.w3.org/TR/activitystreams-vocabulary/ | `specifications/activitypub/w3c-activitystreams-vocabulary.html` | `Person`, `Note`, `Create`, `Follow`, `Accept`, `Reject`, `Like`, `Announce`, collections |
| WebFinger RFC 7033 | https://www.rfc-editor.org/rfc/rfc7033 | `specifications/activitypub/rfc7033-webfinger.html` | `acct:` discovery and JRD responses |
| Mastodon ActivityPub | https://docs.joinmastodon.org/spec/activitypub/ | `specifications/mastodon/activitypub.html` | Mastodon status/profile federation behavior, extensions, payload expectations |
| Mastodon WebFinger | https://docs.joinmastodon.org/spec/webfinger/ | `specifications/mastodon/webfinger.html` | Mastodon-compatible discovery links and profile aliases |
| Mastodon Security | https://docs.joinmastodon.org/spec/security/ | `specifications/mastodon/security.html` | HTTP Signatures, digest verification, secure mode conventions |

### ActivityPub Summary

ActivityPub defines federated social objects and actor endpoints. For dais, the
critical endpoints are:

- `GET /users/:username`: actor document.
- `POST /users/:username/inbox`: signed server-to-server delivery.
- `GET /users/:username/outbox`: activities the requester is authorized to see.
- `GET /users/:username/posts/:post_id`: object dereferencing.
- `GET /users/:username/followers` and `/following`: collection summaries/pages.

ActivityPub itself is flexible about authorization: outbox/object `GET` can show
only objects the requester is authorized to see. For dais, anonymous reads must
only expose explicitly public, non-E2EE material.

### Mastodon-Specific Behavior

Mastodon publishes compatibility behavior beyond core ActivityPub:

- HTTP Signatures and Digest verification are required for practical federation.
- Actor objects should expose a `publicKey` object using the security vocabulary.
- `manuallyApprovesFollowers` marks a locked account.
- Mastodon consumes `Note` and `Question` most directly; other object types are
  transformed best-effort.
- Mastodon documents HTML sanitization behavior for incoming status content.
- Mastodon supports and documents extensions such as `toot:` terms, featured
  collections, featured tags, profile metadata, sensitive content, quote-related
  controls, and follower synchronization.
- Public `Note` payloads should include Mastodon-consumed fields such as
  `content`, `summary`, `sensitive`, `inReplyTo`, `published`, `url`,
  `attributedTo`, `to`, `cc`, `tag`, and `attachment` as applicable.
- Mastodon commonly exposes `replies`, `likes`, and `shares` collections/counts
  for status context. This is currently tracked for dais in issue #86.

## Bluesky and AT Protocol Specifications

Bluesky is an application built on AT Protocol. AT Protocol separates identity,
repositories, lexicons, sync, AppViews, and client APIs. Bluesky-specific social
behavior is mostly defined by `app.bsky.*` lexicons and API documentation, while
the protocol substrate is defined by `atproto.com/specs/*`.

### Official AT Protocol Specs

| Spec | Canonical source | Local copy | Applies to dais |
| --- | --- | --- | --- |
| AT Protocol overview | https://atproto.com/specs/atp | `specifications/atproto/at-protocol.html` | Overall architecture and protocol boundaries |
| Data Model | https://atproto.com/specs/data-model | `specifications/atproto/data-model.html` | JSON/CBOR representations, CIDs, blobs, `$type`, `$link`, `$bytes` |
| Lexicon | https://atproto.com/specs/lexicon | `specifications/atproto/lexicon.html` | Schema language for XRPC APIs and records |
| Cryptography | https://atproto.com/specs/cryptography | `specifications/atproto/cryptography.html` | Signing keys, algorithms, verification conventions |
| Accounts | https://atproto.com/specs/account | `specifications/atproto/accounts.html` | PDS-hosted accounts, lifecycle, migration semantics |
| Repository | https://atproto.com/specs/repository | `specifications/atproto/repository.html` | Signed user repositories, MST structure, commits |
| Media Blobs | https://atproto.com/specs/blob | `specifications/atproto/blobs.html` | Blob upload, CID metadata, lifecycle, security headers |
| Labels | https://atproto.com/specs/label | `specifications/atproto/labels.html` | Moderation labels, signed annotations, self-labels |
| HTTP API (XRPC) | https://atproto.com/specs/xrpc | `specifications/atproto/xrpc.html` | HTTP RPC conventions for `com.atproto.*` and `app.bsky.*` |
| OAuth | https://atproto.com/specs/oauth | `specifications/atproto/oauth.html` | Client authentication and authorization |
| Permissions | https://atproto.com/specs/permission | `specifications/atproto/permissions.html` | Permission scope model |
| Event Stream | https://atproto.com/specs/event-stream | `specifications/atproto/event-stream.html` | WebSocket event stream framing |
| Sync | https://atproto.com/specs/sync | `specifications/atproto/sync.html` | Repository sync and firehose semantics |
| DID | https://atproto.com/specs/did | `specifications/atproto/did.html` | DID identity, DID document requirements |
| Handle | https://atproto.com/specs/handle | `specifications/atproto/handle.html` | DNS handle syntax and resolution |
| Namespaced ID (NSID) | https://atproto.com/specs/nsid | `specifications/atproto/nsid.html` | Lexicon and API namespace identifiers |
| Timestamp ID (TID) | https://atproto.com/specs/tid | `specifications/atproto/tid.html` | Time-sortable identifiers |
| Record Key | https://atproto.com/specs/record-key | `specifications/atproto/record-key.html` | Record key syntax within collections |
| AT URI Scheme | https://atproto.com/specs/at-uri-scheme | `specifications/atproto/at-uri-scheme.html` | `at://` URI references to repos, collections, records |
| Glossary | https://atproto.com/guides/glossary | `specifications/atproto/glossary.html` | Shared terms for PDS, Relay, AppView, repository, firehose |

### Bluesky Application/API References

| Area | Canonical source | Applies to dais |
| --- | --- | --- |
| Bluesky developer docs | https://docs.bsky.app/ | App-specific API usage and client expectations |
| Bluesky get started | https://docs.bsky.app/docs/get-started | Session creation and posting flow examples |
| AT Protocol repository | https://github.com/bluesky-social/atproto | Lexicon source, reference implementations, server/client packages |

Local snapshots for these app-level references:

- `specifications/bluesky/developer-docs.html`
- `specifications/bluesky/get-started.html`
- `specifications/bluesky/atproto-github.html`

The Bluesky docs are API-documentation oriented rather than a single formal
application specification. For endpoint-level app behavior, prefer the current
`app.bsky.*` and `com.atproto.*` Lexicons from the upstream atproto repository.

### AT Protocol Summary

AT Protocol is not ActivityPub. It has a different model:

- Identity is rooted in DIDs, with mutable DNS handles layered on top.
- User data lives in signed repositories hosted by Personal Data Servers (PDS).
- Repositories contain records in Lexicon-defined collections.
- XRPC defines HTTP API shape for protocol and application endpoints.
- Sync/event streams distribute repository updates to Relays and AppViews.
- AppViews provide indexed social behavior such as timelines, search, likes,
  reposts, follower counts, and moderation views.
- `app.bsky.*` lexicons define Bluesky social behavior; AT Protocol itself does
  not define generic social concepts such as follows or avatars.

### Dais AT Protocol Implications

Current dais PDS behavior is a minimal read-oriented compatibility layer. Full
Bluesky parity requires substantially more than the existing endpoints:

- DID and handle resolution for dais accounts.
- A real signed repository implementation with valid commits and CIDs.
- XRPC conformance for `com.atproto.server`, `com.atproto.repo`, and
  `com.atproto.sync`.
- Blob upload and full lifecycle behavior with safe content headers. Dais now
  has a read-only `getBlob` compatibility floor for public image attachments.
- Lexicon-valid `app.bsky.feed.post`, `app.bsky.graph.follow`, profile, like,
  repost, and reply records.
- OAuth/session flow compatible with Bluesky clients.
- Event stream/sync support for repository changes.
- AppView-like read APIs for feeds, author feeds, profiles, notifications, and
  interaction counts.
- Owner-token authenticated compatibility writes for public
  `app.bsky.feed.post` records through `createSession`, `createRecord`, and
  `deleteRecord`. Full OAuth, signed repository commits, and arbitrary record
  collections remain out of scope for the current floor.

## Current Test Hooks

- `npm run test:activitypub-conformance` checks a subset of ActivityPub,
  Mastodon, dais privacy, and ATProto public-read behavior.
- `npm run test:bluesky-conformance` checks the current PDS/AppView
  compatibility floor for identity, repo metadata, public feed records,
  owner-token public post writes/deletes, public image blob reads, search,
  profiles, notifications, graph reads, privacy filtering, and sync guidance.
- The conformance runner should grow alongside this document.
- Gaps found by the runner should be filed as GitHub issues under epic #70.
