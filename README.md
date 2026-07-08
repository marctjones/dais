# dais

dais is a single-user, private-by-default social server for Cloudflare. It
federates over ActivityPub, treats Bluesky / AT Protocol as the public broadcast
surface, and is moving toward end-to-end encrypted private messages over open
federation.

Live instance: `@social@dais.social`

## Current Shape

- Production web: `https://dais.social`
- ActivityPub/Mastodon-compatible origin: `https://social.dais.social`
- AT Protocol PDS origin: `https://pds.dais.social`
- Independent test instance: `@social@skpt.cl`
- Independent test ActivityPub origin: `https://social.skpt.cl`
- Independent test AT Protocol PDS origin: `https://pds.skpt.cl`
- Rust core: `core/`
- Cloudflare Workers: `platforms/cloudflare/workers/`
- Cloudflare bindings: `platforms/cloudflare/bindings/`
- Rust CLI/TUI client: `client/`
- Shared owner-client models: `client-core/`
- Native Slint owner app: `apps/dais-desk/`
- D1 migrations: `cli/migrations/`

The old Python CLI and legacy `workers/` tree have been retired. Use the Rust
client and the core-based Cloudflare worker tree.

## Current Capabilities

- Private-by-default posting: followers-only is the default for CLI/TUI posts.
- Public broadcast is explicit with `--public` or `--visibility public`.
- ActivityPub federation includes WebFinger, actor/outbox/inbox surfaces,
  locked-profile signaling, public post dereference, private/E2EE anonymous
  denial, follower-only reads, replies/likes/boost metadata, and delivery queue
  processing. Cross-instance replies between `social.dais.social` and
  `social.skpt.cl` are verified to preserve remote `inReplyTo` targets and
  surface in the receiving owner thread and notification views.
- Mastodon API compatibility has a growing compatibility floor: instance
  metadata, app registration/token compatibility stubs, account reads,
  followers/following, public/home timelines, individual public status reads,
  status context, notifications reads, authenticated status creation, and
  private/public gating.
- AT Protocol support includes PDS/AppView-style public read endpoints and Rust
  client Bluesky public operations.
- E2EE support includes a dais `encryptedMessage` envelope, Rust CLI
  encrypt/decrypt helpers, keyless/split/trusted fallback modes for
  Mastodon-style recipients, owner device publication, peer discovery/trust,
  local private-key storage/export, and owner API send/decrypt commands for the
  v1 fallback path and MLS v2. The live `social.dais.social` and independent
  `social.skpt.cl` instances pass MLS v2 device publication, bidirectional
  owner-DM delivery/decrypt, audience-list group delivery/decrypt, two-device
  recipient delivery/decrypt for one actor, and removed-device decrypt-failure
  smoke tests, including the 2026-07-08 strict production/skpt release gate.
- Rich ActivityPub object support includes ActivityStreams `Note`, `Article`,
  `Document`, and `Event` objects from the Rust CLI, including title/summary,
  event time, and location metadata while preserving Mastodon fallback status
  text.
- Managed actor mode can publish the local ActivityPub actor as `Person`,
  `Group`, or `Organization` for personal, community, and small-business
  deployment patterns, and can update display-name/icon/header metadata from
  the Rust CLI.
- Rust owner tooling includes media upload/attachment helpers, moderation and
  closed-network allowlist controls, delivery/follower review, expanded reports,
  and a TUI for day-to-day operation. The HTTPS owner API exposes token-gated
  owner reads and compose for GUI/mobile clients. Dais Desk uses the same owner
  API and now prioritizes replies/mentions as conversational rows while keeping
  likes and boosts as lightweight activity.
- Public source subscriptions can ingest standards-based RSS/Atom feeds and
  JSON API sources with `articles[]` or `items[]` into a private reader item
  model with rights-policy metadata; scheduled Cloudflare refresh stores
  metadata/excerpts only and never reposts automatically.
- Private watches can follow owner-selected RSS, ActivityPub, and Bluesky public
  targets without creating remote follow relationships, and public search is
  explicit through Bluesky and ActivityPub provider buckets with sensitive-query
  guardrails.
- Private community/group primitives are implemented through owner audience
  groups with explicit `audience` vs `private_group` purpose,
  private-by-default membership visibility, posting policy metadata, direct
  group posting, and Dais Desk/CLI visibility labels.

Mastodon parity is not complete. Dais is currently best described as
Mastodon-readable with a growing compatibility API, not a full Mastodon server
replacement.

## Product Direction

The source of truth for the product is:

- [docs/POSITIONING.md](docs/POSITIONING.md)
- [docs/design/PRIVATE_MODE.md](docs/design/PRIVATE_MODE.md)
- [docs/design/PROTOCOL_ADAPTERS.md](docs/design/PROTOCOL_ADAPTERS.md)
- [docs/design/E2EE_WIRE_FORMAT.md](docs/design/E2EE_WIRE_FORMAT.md)
- [docs/guides/MANAGED_HOSTING.md](docs/guides/MANAGED_HOSTING.md)
- [docs/guides/MIGRATION_IMPORTS.md](docs/guides/MIGRATION_IMPORTS.md)
- [docs/guides/MODEL_ALLOCATION.md](docs/guides/MODEL_ALLOCATION.md)

GitHub issues under epic #70 track roadmap, decisions, and active work.

## Rust Client

Run the client locally:

```bash
cargo run --manifest-path client/Cargo.toml -- --help
```

Common commands:

```bash
cargo run --manifest-path client/Cargo.toml -- post create "private by default"
cargo run --manifest-path client/Cargo.toml -- post create "public broadcast" --public --protocol both
cargo run --manifest-path client/Cargo.toml -- post create "long-form private note" --object-type article --title "Long-form title" --summary "Short abstract" --protocol activitypub --remote
cargo run --manifest-path client/Cargo.toml -- media attachment https://social.dais.social/media/example.png --kind Image --media-type image/png --name example
cargo run --manifest-path client/Cargo.toml -- post create "post with media" --attachment '{"type":"Image","url":"https://social.dais.social/media/example.png","mediaType":"image/png"}' --remote
cargo run --manifest-path client/Cargo.toml -- events create "Dinner" --starts-at 2026-06-12T18:00:00Z --location "Home" --remote
cargo run --manifest-path client/Cargo.toml -- actors set-type organization --remote
cargo run --manifest-path client/Cargo.toml -- actors update --display-name "dais" --summary "Private-by-default social server" --remote
cargo run --manifest-path client/Cargo.toml -- moderation status --remote
cargo run --manifest-path client/Cargo.toml -- reports summary --remote
cargo run --manifest-path client/Cargo.toml -- sources add rss https://www.w3.org/blog/news/feed/ --title "W3C News" --remote
cargo run --manifest-path client/Cargo.toml -- sources add api https://api.example.com/articles --title "Licensed API" --api-secret-name EXAMPLE_API_TOKEN --remote
cargo run --manifest-path client/Cargo.toml -- sources refresh --remote
cargo run --manifest-path client/Cargo.toml -- sources items --remote
cargo run --manifest-path client/Cargo.toml -- owner audience-lists
cargo run --manifest-path client/Cargo.toml -- owner audience-save --id close-friends --name "Close Friends" --group-type private_group --member https://example.social/users/alice
cargo run --manifest-path client/Cargo.toml -- owner post-create "private group note" --visibility direct --audience-list-id close-friends
cargo run --manifest-path client/Cargo.toml -- timeline home --remote
cargo run --manifest-path client/Cargo.toml -- friends list --remote
cargo run --manifest-path client/Cargo.toml -- tui --remote
```

Private/followers visibility is the default. Public posting is explicit.

## Owner App

The first-party desktop owner app lives in `apps/dais-desk` and reuses Rust
models plus HTTP owner API calls from `client-core`. It is a native Slint owner
workspace with live snapshots, private-by-default compose, timelines,
notifications, DMs, public discovery, follows, followers, friends, watches,
audience groups, moderation, deliveries, diagnostics, settings, and local
multi-account profiles.

```bash
cargo run --manifest-path apps/dais-desk/Cargo.toml
cargo test --manifest-path apps/dais-desk/Cargo.toml
```

Production owner API access requires the router worker secret
`OWNER_API_TOKEN`. The local Dais Desk settings file stores instance URLs and
owner tokens in the platform app configuration directory. On macOS development
builds this is under:

```text
~/Library/Application Support/social.dais.desk/owner-settings.json
```

## Development

Install local prerequisites:

```bash
./scripts/setup-dev.sh
```

Seed local D1 state:

```bash
./scripts/seed-local-db.sh
```

Run checks:

```bash
cargo test --manifest-path core/Cargo.toml
cargo check --manifest-path client/Cargo.toml
cargo check --manifest-path platforms/cloudflare/workers/router/Cargo.toml
cargo check --manifest-path platforms/cloudflare/workers/landing/Cargo.toml
cargo test --manifest-path conformance/Cargo.toml -- --nocapture
```

Run the active server release gate:

```bash
scripts/release-server.sh --strict
```

Managed-hosting operations are tracked in
[docs/guides/MANAGED_HOSTING.md](docs/guides/MANAGED_HOSTING.md). The current
repeatable operational tools are:

```bash
scripts/provision-managed-instance.sh --slug example-family --domain example.com
scripts/smoke-managed-instance.sh --domain skpt.cl --activitypub-domain social.skpt.cl --pds-domain pds.skpt.cl --owner-token-file /private/tmp/dais-skpt-owner-token.txt
scripts/managed-health-check.sh --domain skpt.cl --activitypub-domain social.skpt.cl --pds-domain pds.skpt.cl --owner-token-file /private/tmp/dais-skpt-owner-token.txt
scripts/verify-backup-restore.sh --self-test
scripts/import-preview.rb --format opml --file subscriptions.opml
```

Use `scripts/release-server.sh --plan` to print the exact gate plan without
running it. Use
`scripts/release-server.sh --deploy --strict --bluesky-conformance --mastodon-conformance`
when preparing a production/skpt release after the build and smoke gates pass.
Deploy mode applies the selected idempotent D1 schema updates through
`scripts/apply-release-d1-migrations.sh` before uploading Workers and runs the
required independent-instance, cross-instance E2EE, and cross-instance MLS live
smokes.

Live independent-instance smoke:

```bash
scripts/smoke-local-mls.sh
scripts/audit-skpt-independence.sh
scripts/smoke-skpt-instance.sh
scripts/smoke-cross-instance-e2ee.sh
scripts/smoke-cross-instance-mls.sh
```

`scripts/smoke-local-mls.sh` runs the no-token OpenMLS gate: device material,
1:1 send/decrypt, malformed/wrong protocol failure, small-group add/remove, and
removed-member decrypt failure.
`scripts/audit-skpt-independence.sh` verifies the `skpt` worker configs use
distinct worker names, D1, R2, queues, routes, and domains.
`scripts/smoke-skpt-instance.sh` verifies the independent `skpt.cl` deployment.
`scripts/smoke-cross-instance-e2ee.sh` verifies both actors and, when both
`DAIS_OWNER_TOKEN`/`DAIS_OWNER_TOKEN_FILE` and
`SKPT_OWNER_TOKEN`/`SKPT_OWNER_TOKEN_FILE` are available, initializes missing
devices, discovers and trusts peers, sends encrypted messages in both
directions, and decrypts them with the retained private keys. Without both
owner tokens it reports the missing prerequisite and skips the send/decrypt
path. Set `REQUIRE_FULL=1` to make missing prerequisites fail a release gate.
Strict mode also requires both delivery admin tokens so queued delivery
processing cannot silently degrade into exploratory skip behavior.
`scripts/smoke-cross-instance-mls.sh` runs the live MLS v2 gate between
`social.dais.social` and `social.skpt.cl`: actor fetch, MLS device publication,
mutual discovery/trust, bidirectional 1:1 send/decrypt, audience-list group
send/decrypt, two trusted recipient devices for one actor, removed-device
decrypt failure after peer revocation, and delivery queue processing when
delivery admin tokens are available. This is the live equivalent topology for
broader MLS lifecycle coverage until a third independently managed actor is
available. Set `REQUIRE_FULL=1` for release gates that must fail if either owner
token or either delivery admin token is unavailable.

Worker builds use current `worker-build` with the rustup toolchain path set in
each worker `wrangler.toml`.

## Deploy

Use the repository deploy script:

```bash
scripts/deploy.sh build
scripts/deploy.sh deploy --env production --yes
scripts/deploy.sh deploy --env skpt --yes
```

Default deploys target only the active `landing` and `router` workers. The old
split workers remain in the repository for compatibility and rollback; deploy
them only when a task explicitly requires it.

Deploy individual active workers when needed:

```bash
scripts/deploy.sh deploy --env production --only landing --yes
scripts/deploy.sh deploy --env production --only router --yes
```

Build or deploy legacy split workers explicitly:

```bash
scripts/deploy.sh build --include-legacy
scripts/deploy.sh deploy --env production --include-legacy --yes
```

See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for the longer operational guide.

## Documentation

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/ROADMAP.md](docs/ROADMAP.md)
- [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)
- [docs/guides/DEVELOPMENT.md](docs/guides/DEVELOPMENT.md)
- [docs/guides/OPERATIONAL_RUNBOOK.md](docs/guides/OPERATIONAL_RUNBOOK.md)
- [docs/guides/DESK_PRODUCT_COMPLETENESS_AUDIT.md](docs/guides/DESK_PRODUCT_COMPLETENESS_AUDIT.md)
- [docs/guides/MODEL_ALLOCATION.md](docs/guides/MODEL_ALLOCATION.md)
- [docs/guides/PRIVACY_GUIDE.md](docs/guides/PRIVACY_GUIDE.md)
- [docs/guides/FEDERATION_GUIDE.md](docs/guides/FEDERATION_GUIDE.md)
- [docs/reference/API_DOCUMENTATION.md](docs/reference/API_DOCUMENTATION.md)

Historical v1.0/v1.1 snapshots live in [docs/archive](docs/archive/).
