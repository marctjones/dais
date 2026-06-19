# dais Owner Tauri App

The first-party owner app lives in `apps/owner-tauri`. It is a Tauri v2 desktop
shell intended to share Rust owner-client models with the CLI/TUI through
`client-core`, and to become Android-capable later without a UI rewrite.

Current status:

- Adaptive desktop/narrow layout for Home, Posts, Sources, Notifications,
  Followers, Watches, Profile, Moderation, Deliveries, Settings, and
  Diagnostics.
- Local multi-account settings storage for Dais instance profiles, with an
  active-account sidebar switcher and per-instance owner tokens.
- Shared Rust `dais-client-core` models and `OwnerApiClient` HTTP calls for
  snapshots, compose, profile updates, and follower status updates.
- Live owner snapshot loading and private-by-default post creation when an owner
  token is configured.
- Live follower management for pending, approved, and rejected follower rows.
- Live public account/profile configuration for the ActivityPub actor, HTML
  profile, and Mastodon account API output.
- Approved-follower selection in compose for direct ActivityPub posts.
- Post-detail operator actions for replies, likes, boosts, deletion, and media
  revocation.
- Live Watch management for private monitoring of public RSS, Atom,
  ActivityPub, and Bluesky posts without remote follow or subscription records.
- Local preview data when no token is configured or the owner API is
  unreachable.

Run the frontend shell:

```bash
cd apps/owner-tauri
npm install
npm run build
```

Run the Tauri app locally without starting a frontend dev server:

```bash
cd apps/owner-tauri
npm run tauri:run
```

Build the desktop bundle:

```bash
cd apps/owner-tauri
npm run tauri:build
```

On macOS this creates:

```text
apps/owner-tauri/src-tauri/target/release/bundle/macos/dais owner.app
```

Configure production owner API access:

```bash
cd platforms/cloudflare/workers/router
printf '%s' '<random-token>' | npx wrangler secret put OWNER_API_TOKEN --env production
```

The Tauri app stores local account profiles in the platform app configuration
directory. Existing single-account settings are migrated automatically into the
first account profile. On macOS development builds this is under:

```text
~/Library/Application Support/social.dais.owner/owner-settings.json
```

Use the Settings screen to add a label, instance URL, and owner token for each
Dais instance. Switching accounts changes the active owner API target for all
client workflows, including timelines, compose, replies, following, watches,
profile edits, moderation, diagnostics, and delivery inspection.

Android readiness notes:

- Keep UI behavior responsive at 360px width.
- Do not add workflows that require Wrangler, local D1 files, or Cloudflare
  account credentials in the app.
- Owner workflows should go through the scoped owner API, with revocable tokens
  stored by the platform once scoped token issuance is implemented.
- Android packaging will require the Android SDK/NDK and the Tauri Android
  target setup, but that should be packaging work, not an app rewrite.
