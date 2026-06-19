# Dais Desk Native App

The first-party owner app lives in `apps/dais-desk`. It is a Rust-native Slint
desktop client that shares owner-client models and HTTP API calls with the
CLI/TUI through `client-core`.

The information architecture is documented in
`docs/design/DAIS_DESK_INFORMATION_ARCHITECTURE.md`: Home for daily social work,
People for relationships and discovery, and Server for operator tasks. Reusable
interaction rules and UI primitives are documented in
`docs/design/DAIS_DESK_DESIGN_SYSTEM.md`. UI release gates are documented in
`docs/guides/UI_RELEASE_GATES.md`.

Current status:

- Native Slint shell for Home, People, and Server workflows.
- Local multi-account settings storage for Dais instance profiles, with
  per-instance owner tokens.
- Shared Rust `dais-client-core` models and `OwnerApiClient` HTTP calls.
- Live owner snapshot loading with fixture preview mode when no token is
  configured or the owner API is unreachable.
- Private-by-default compose, replies, likes, boosts, deletion, media metadata,
  direct-recipient warnings, and ActivityPub/Bluesky protocol routing controls.
- Timelines, notifications, DMs, saved/draft rows, public discovery, follows,
  followers, following, friends, watches, audience groups, blocks, moderation,
  deliveries, diagnostics, profile/settings, stats, and account management.
- Accessibility metadata for source-list controls and row cards, with automated
  Slint interaction tests.
- Native visual smoke screenshots for Home, People/Followers, and
  Server/Accounts.

Run the app locally:

```bash
cargo run --manifest-path apps/dais-desk/Cargo.toml
```

Run the required Dais Desk smoke gate:

```bash
cargo test --manifest-path apps/dais-desk/Cargo.toml
```

This runs the Rust unit tests, Slint interaction tests, and the native visual
smoke test. The visual smoke writes screenshots to:

```text
apps/dais-desk/target/dais-desk-screenshots/
```

Configure production owner API access:

```bash
cd platforms/cloudflare/workers/router
printf '%s' '<random-token>' | wrangler secret put OWNER_API_TOKEN --env production
```

Dais Desk stores local account profiles in the platform app configuration
directory. On macOS development builds this is under:

```text
~/Library/Application Support/social.dais.desk/owner-settings.json
```

On first launch, Dais Desk also checks the retired owner-app settings path
`~/Library/Application Support/social.dais.owner/owner-settings.json` and
migrates the saved owner API token into the Desk settings file when the Desk
settings file does not exist yet.

Use the Accounts & Tokens screen to add a label, instance URL, and owner token
for each Dais instance. Switching accounts changes the active owner API target
for reads, compose, replies, follows, watches, moderation, diagnostics, delivery
inspection, and settings.
