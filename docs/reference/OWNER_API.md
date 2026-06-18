# dais Owner API

The owner API is the HTTPS boundary for first-party GUI/mobile clients. It lets
the Tauri app, future Android app, CLI, and TUI converge on one secure client
surface instead of requiring Wrangler, raw D1 access, or Cloudflare admin
credentials.

Base URL:

```text
https://social.dais.social/api/dais/owner
```

Authentication:

```http
Authorization: Bearer <OWNER_API_TOKEN>
```

Production fails closed when `OWNER_API_TOKEN` is not configured on the router
worker. Anonymous requests receive `401`.

Implemented endpoints:

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/snapshot` | Combined owner app state for Tauri/client startup. |
| `GET` | `/profile` | Public actor/account profile metadata. |
| `POST` | `/profile` | Update display name, actor type, summary, avatar/icon URL, and header image URL. |
| `GET` | `/posts` | Recent local owner posts, including private and encrypted metadata. |
| `POST` | `/posts` | Create a private-by-default ActivityPub owner post, with optional direct recipients, reply target, encryption flag, and ActivityStreams attachments. |
| `POST` | `/media` | Upload public or private media and return attachment JSON for post creation. Private uploads may include `expires_in_seconds`, up to 30 days. |
| `POST` | `/media/revoke` | Delete a previously uploaded media URL. |
| `GET` | `/timeline/home` | Signed-in home timeline from accepted follows. |
| `GET` | `/followers` | Local follower rows. |
| `POST` | `/followers/status` | Mark a follower `approved`, `pending`, or `rejected`. |
| `GET` | `/following` | Local following rows. |
| `GET` | `/notifications` | Local notifications. |
| `POST` | `/notifications/read` | Mark a notification as read. |
| `GET` | `/deliveries` | ActivityPub delivery jobs. |
| `GET` | `/search?q=<term>&scope=local\|public\|all` | Operator search. `local` searches Dais posts, follows, sources, and reader items. `public` queries explicit public Bluesky and Mastodon-compatible providers. `all` returns both. Sensitive-looking public queries return `public_search_guard.blocked=true` and skip provider calls unless `confirm_public_sensitive=true` is supplied. |
| `GET` | `/sources` | Public source subscriptions and private reader items. |
| `GET` | `/watches` | Private Watch subscriptions and harvested public posts. |
| `POST` | `/watches` | Add an RSS, Atom, ActivityPub, or Bluesky public Watch target without creating a remote follow, approval request, graph record, or notification subscription. |
| `POST` | `/watches/refresh` | Refresh one Watch by `id`, or all active Watch targets when no `id` is supplied. |
| `DELETE` | `/watches/:id` | Remove a Watch subscription and its reader items. |
| `GET` | `/moderation` | Closed-network, block, allowlist, and follower policy state. |
| `GET` | `/diagnostics` | Owner API, private default, ActivityPub, and delivery health. |

Watch add body:

```json
{
  "watch_type": "activitypub_actor",
  "target": "@nasa@social.nasa.gov",
  "title": "NASA ActivityPub",
  "cadence_minutes": 60,
  "private_reader_only": true,
  "excerpt_only": true,
  "link_required": true,
  "attribution_required": true,
  "image_allowed": false,
  "full_text_allowed": false
}
```

Supported `watch_type` values are `rss`, `atom`, `activitypub_actor`,
`activitypub_object`, `bluesky_actor`, and `bluesky_post`. Watch targets are
private local reader state. Dais fetches only public posts using normal public
protocol endpoints: unsigned ActivityPub public GETs, Bluesky public AppView
reads, and RSS/Atom feed GETs. It does not send ActivityPub `Follow`, Bluesky
graph follow records, approval requests, WebSub subscription requests, or remote
notifications. Remote servers may still observe ordinary HTTP fetches in access
logs.

Known gaps:

- Tokens are currently a single Cloudflare Worker secret. Scoped tokens,
  revocation, rotation UI, and per-scope enforcement are still planned.
- Compose creates text posts, queues ActivityPub deliveries for followers-only
  and direct posts, and accepts ActivityStreams attachment JSON from the owner
  media upload endpoint. Rich non-`Note` objects and poll creation remain
  Mastodon API or local CLI surfaces.
- Search responses include local arrays (`posts`, `users`, `sources`,
  `source_items`), public arrays (`public_posts`, `public_actors`,
  `provider_errors`), and `public_search_guard` so clients can show when a
  public-provider query was paused for operator confirmation.
- The Rust CLI can exercise live owner API compose with
  `dais owner post-create`, media uploads with `dais owner media-upload`, and
  media revocation with `dais owner media-revoke`. It can manage private Watch
  targets with `dais owner watches`, `dais owner watch-add`, `dais owner
  watch-refresh`, and `dais owner watch-remove`. It can opt into public search
  with `dais owner search --scope public <term>` and can confirm a sensitive
  public search with `--confirm-public-sensitive`.
- Private media capability URLs can expire automatically, but recipient-bound
  authorized-fetch media access remains future hardening.
- Profile updates currently cover the fields reflected in ActivityPub actor
  JSON, the HTML profile page, and Mastodon account reads. Custom profile
  fields and per-field visibility controls remain future work.
- Destructive actions such as delete, block, and source removal should be added
  only after scope enforcement is in place.
