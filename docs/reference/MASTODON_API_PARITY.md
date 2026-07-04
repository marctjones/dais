# Mastodon API Parity Matrix

Status: v0.27 working matrix. This tracks the practical third-party Mastodon
client compatibility surface for single-user dais. The ActivityPub server-to-
server surface is tracked separately in v0.26.

Private-by-default rule: public Mastodon API endpoints expose only public,
non-encrypted dais posts. Authenticated endpoints may operate on owner state, but
must not publish followers-only, direct, or E2EE content as public data.

## Implemented Compatibility Floor

| Area | Endpoints | Status |
| --- | --- | --- |
| Instance | `GET /api/v1/instance`, `GET /api/v2/instance` | Implemented |
| Apps/OAuth | `POST /api/v1/apps`, `GET /oauth/authorize`, `POST /oauth/token`, `POST /oauth/revoke`, `GET /.well-known/oauth-authorization-server`, `GET /.well-known/openid-configuration` | Compatibility shape only; `/oauth/token` returns an explicit non-authenticating `owner-token-required` placeholder with `dais_authentication: "owner_token_required"` until a local owner consent flow exists, and production access requires an owner-provisioned bearer token |
| Discovery | `GET /.well-known/nodeinfo`, `GET /nodeinfo/2.0` | Implemented for client/server metadata discovery |
| Account | `GET /api/v1/accounts/verify_credentials`, `PATCH /api/v1/accounts/update_credentials`, `GET /api/v1/accounts/:id` | Implemented for single local account |
| Graph | `GET /api/v1/accounts/:id/followers`, `GET /api/v1/accounts/:id/following`, `GET /api/v1/accounts/relationships` | Implemented |
| Account/client probes | `GET /api/v1/follow_requests`, `POST /api/v1/follow_requests/:id/authorize`, `POST /api/v1/follow_requests/:id/reject`, `GET /api/v1/suggestions`, `DELETE /api/v1/suggestions/:id`, `GET /api/v1/endorsements`, `GET /api/v1/featured_tags`, `GET /api/v1/followed_tags` | Implemented as empty/safe single-user compatibility surfaces |
| Relationship writes | `POST /api/v1/accounts/:id/follow`, `unfollow`, `block`, `unblock`, `mute`, `unmute` | Implemented; mute/unmute persist local relationship state |
| Timelines | `GET /api/v1/timelines/public`, `GET /api/v1/timelines/home`, `GET /api/v1/accounts/:id/statuses` | Implemented with privacy filtering and `max_id`/`since_id`/`min_id` cursors |
| Status reads | `GET /api/v1/statuses/:id`, `GET /api/v1/statuses/:id/context`, `GET /api/v1/statuses/:id/source` | Implemented; context includes local public ancestors and direct reply descendants, source supports edit-capable clients, and status JSON includes mention/tag arrays |
| Status writes | `POST /api/v1/statuses`, `PUT/PATCH /api/v1/statuses/:id`, `DELETE /api/v1/statuses/:id` | Implemented; deletes queue ActivityPub `Delete` to followers |
| Interactions | `POST /api/v1/statuses/:id/favourite`, `unfavourite`, `reblog`, `unreblog`, `GET /api/v1/favourites` | Implemented for local status state |
| Media | `POST /api/v1/media`, `POST /api/v2/media`, `GET /api/v1/media/:id`, `PUT/PATCH /api/v1/media/:id` | Implemented for public image/video uploads plus metadata read/update |
| Polls | ActivityPub `Question` with `oneOf`/`anyOf` options; Mastodon API `poll` parameters on `POST /api/v1/statuses` | Implemented for CLI-created posts, server-to-server object rendering, and Mastodon API status creation |
| Notifications | `GET /api/v1/notifications`, `POST /api/v1/notifications/:id/dismiss`, `POST /api/v1/notifications/clear` | Implemented |
| Search | `GET /api/v1/search`, `GET /api/v2/search` | Implemented for public local statuses and ActivityPub actor lookup; status results support `max_id`/`since_id`/`min_id` cursors |
| Client lists | `GET /api/v1/filters`, `GET /api/v2/filters`, `GET /api/v1/lists`, `GET /api/v1/bookmarks`, `GET /api/v1/conversations`, `GET/POST /api/v1/markers`, `GET /api/v1/scheduled_statuses` | Implemented as empty/compatible single-user surfaces |
| Moderation | `GET /api/v1/blocks`, `GET /api/v1/mutes`, `GET/POST/DELETE /api/v1/domain_blocks`, `POST /api/v1/reports` | Implemented |
| Discovery/probes | `GET /api/v1/custom_emojis`, `GET /api/v1/announcements`, `GET /api/v1/directory`, `GET /api/v1/trends`, `GET /api/v1/trends/statuses`, `GET /api/v1/trends/tags`, `GET /api/v1/trends/links` | Implemented as empty/safe compatibility surfaces |
| Streaming | `GET /api/v1/streaming/*` | SSE-compatible fallback stream with reconnect guidance; clients should still poll for new data |

## Intentional Limits

- Full multi-user admin APIs are out of scope for single-user dais.
- The OAuth token endpoint intentionally does not reveal or mint the production
  owner token. Until a real local consent screen exists, third-party clients need
  an owner-provisioned bearer token. The placeholder `owner-token-required`
  response is covered by conformance, includes
  `dais_authentication: "owner_token_required"`, and must not authenticate.
- Private, direct, and E2EE posts are not exposed through public Mastodon
  timelines or public status reads.
- Mute/unmute persist local owner-side account mute state, and
  `GET /api/v1/mutes` lists those muted accounts for Mastodon-compatible
  clients.
- Follow requests, suggestions, endorsements, featured tags, followed tags,
  scheduled statuses, announcements, directory, and trends return empty shapes
  because dais is a single-user private-by-default server without global
  discovery or multi-user moderation queues.
- Streaming returns a valid SSE connection frame and reconnect hint, but does
  not yet push live updates; clients should poll for new data.

## Release Gates

Run before closing v0.27 slices:

```bash
cd platforms/cloudflare/workers/router
cargo check --target wasm32-unknown-unknown
cd ../../../..
DAIS_CONFORMANCE_ONLY=mastodon-api cargo test --manifest-path conformance/Cargo.toml -- --nocapture
```

For authenticated production checks:

```bash
DAIS_MASTODON_BEARER_TOKEN="$OWNER_API_TOKEN" \
DAIS_CONFORMANCE_ONLY=mastodon-api \
cargo test --manifest-path conformance/Cargo.toml -- --nocapture
```

For a third-party-client-shaped smoke that uses form-encoded OAuth/status
requests and multipart media upload:

```bash
DAIS_MASTODON_BEARER_TOKEN="$OWNER_API_TOKEN" \
DAIS_CONFORMANCE_ONLY=mastodon-client-smoke \
cargo test --manifest-path conformance/Cargo.toml -- --nocapture
```
