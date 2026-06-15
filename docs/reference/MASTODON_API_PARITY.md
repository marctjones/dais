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
| Apps/OAuth | `POST /api/v1/apps`, `GET /oauth/authorize`, `POST /oauth/token`, `POST /oauth/revoke`, `GET /.well-known/oauth-authorization-server`, `GET /.well-known/openid-configuration` | Compatibility shape; production access still requires an owner-provisioned bearer token |
| Discovery | `GET /.well-known/nodeinfo`, `GET /nodeinfo/2.0` | Implemented for client/server metadata discovery |
| Account | `GET /api/v1/accounts/verify_credentials`, `PATCH /api/v1/accounts/update_credentials`, `GET /api/v1/accounts/:id` | Implemented for single local account |
| Graph | `GET /api/v1/accounts/:id/followers`, `GET /api/v1/accounts/:id/following`, `GET /api/v1/accounts/relationships` | Implemented |
| Relationship writes | `POST /api/v1/accounts/:id/follow`, `unfollow`, `block`, `unblock`, `mute`, `unmute` | Implemented; mute/unmute are compatibility no-ops |
| Timelines | `GET /api/v1/timelines/public`, `GET /api/v1/timelines/home`, `GET /api/v1/accounts/:id/statuses` | Implemented with privacy filtering and `max_id`/`since_id`/`min_id` cursors |
| Status reads | `GET /api/v1/statuses/:id`, `GET /api/v1/statuses/:id/context` | Implemented; context includes local public ancestors and direct reply descendants, and status JSON includes mention/tag arrays |
| Status writes | `POST /api/v1/statuses`, `PUT/PATCH /api/v1/statuses/:id`, `DELETE /api/v1/statuses/:id` | Implemented; deletes queue ActivityPub `Delete` to followers |
| Interactions | `POST /api/v1/statuses/:id/favourite`, `unfavourite`, `reblog`, `unreblog`, `GET /api/v1/favourites` | Implemented for local status state |
| Media | `POST /api/v1/media`, `POST /api/v2/media` | Implemented for public image and video uploads |
| Polls | ActivityPub `Question` with `oneOf`/`anyOf` options; Mastodon API `poll` parameters on `POST /api/v1/statuses` | Implemented for CLI-created posts, server-to-server object rendering, and Mastodon API status creation |
| Notifications | `GET /api/v1/notifications`, `POST /api/v1/notifications/:id/dismiss`, `POST /api/v1/notifications/clear` | Implemented |
| Search | `GET /api/v1/search`, `GET /api/v2/search` | Implemented for public local statuses and ActivityPub actor lookup; status results support `max_id`/`since_id`/`min_id` cursors |
| Client lists | `GET /api/v1/filters`, `GET /api/v2/filters`, `GET /api/v1/lists`, `GET /api/v1/bookmarks`, `GET /api/v1/conversations`, `GET/POST /api/v1/markers` | Implemented as empty/compatible single-user surfaces |
| Moderation | `GET /api/v1/blocks`, `GET /api/v1/mutes`, `POST /api/v1/reports` | Implemented |
| Streaming | `GET /api/v1/streaming/*` | Compatibility event-stream placeholder |

## Intentional Limits

- Full multi-user admin APIs are out of scope for single-user dais.
- The OAuth token endpoint intentionally does not reveal or mint the production
  owner token. Until a real local consent screen exists, third-party clients need
  an owner-provisioned bearer token.
- Private, direct, and E2EE posts are not exposed through public Mastodon
  timelines or public status reads.
- Mute/unmute return relationship-compatible state but do not yet maintain a
  separate mute table.
- Streaming is a compatibility placeholder; clients should fall back to polling.

## Release Gates

Run before closing v0.27 slices:

```bash
node --check platforms/cloudflare/workers/router/src/index.js
npm run test:mastodon-api-conformance
```

For authenticated production checks:

```bash
DAIS_MASTODON_BEARER_TOKEN="$OWNER_API_TOKEN" npm run test:mastodon-api-conformance
```
