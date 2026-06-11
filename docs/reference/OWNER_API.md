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
| `GET` | `/posts` | Recent local owner posts, including private and encrypted metadata. |
| `POST` | `/posts` | Create a private-by-default ActivityPub owner post. |
| `GET` | `/timeline/home` | Signed-in home timeline from accepted follows. |
| `GET` | `/followers` | Local follower rows. |
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
- Compose stores the post and preserves private/public routing intent, but live
  delivery queue fanout from this endpoint still needs the same delivery wiring
  used by the Rust CLI.
- Destructive actions such as delete, block, reject follower, and source removal
  should be added only after scope enforcement is in place.
