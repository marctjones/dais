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
| `POST` | `/posts` | Create a private-by-default ActivityPub owner post. |
| `GET` | `/timeline/home` | Signed-in home timeline from accepted follows. |
| `GET` | `/followers` | Local follower rows. |
| `POST` | `/followers/status` | Mark a follower `approved`, `pending`, or `rejected`. |
| `GET` | `/following` | Local following rows. |
| `GET` | `/notifications` | Local notifications. |
| `POST` | `/notifications/read` | Mark a notification as read. |
| `GET` | `/deliveries` | ActivityPub delivery jobs. |
| `GET` | `/sources` | Public source subscriptions and private reader items. |
| `GET` | `/moderation` | Closed-network, block, allowlist, and follower policy state. |
| `GET` | `/diagnostics` | Owner API, private default, ActivityPub, and delivery health. |

Known gaps:

- Tokens are currently a single Cloudflare Worker secret. Scoped tokens,
  revocation, rotation UI, and per-scope enforcement are still planned.
- Compose creates plain text posts and queues ActivityPub deliveries for
  followers-only and direct posts. Rich objects, media attachments, and E2EE
  compose remain CLI/TUI-first until the owner API surface is expanded.
- Profile updates currently cover the fields reflected in ActivityPub actor
  JSON, the HTML profile page, and Mastodon account reads. Custom profile
  fields and per-field visibility controls remain future work.
- Destructive actions such as delete, block, and source removal should be added
  only after scope enforcement is in place.
