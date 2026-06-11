# dais Owner Tauri App

The first-party owner app lives in `apps/owner-tauri`. It is a Tauri v2 desktop
shell intended to share Rust owner-client models with the CLI/TUI through
`client-core`, and to become Android-capable later without a UI rewrite.

Current status:

- Adaptive desktop/narrow layout for Home, Posts, Sources, Notifications,
  Followers, Profile, Moderation, Deliveries, Settings, and Diagnostics.
- Local settings storage for instance URL and owner token.
- Shared Rust `dais-client-core` models and `OwnerApiClient` HTTP calls for
  snapshots and compose.
- Live owner snapshot loading and private-by-default post creation when an owner
  token is configured.
- Local preview data when no token is configured or the owner API is
  unreachable.

Run the frontend shell:

```bash
cd apps/owner-tauri
npm install
npm run build
```

Run the Tauri app in development:

```bash
cd apps/owner-tauri
npm run tauri:dev
```

Build the desktop bundle:

```bash
cd apps/owner-tauri
npm run tauri:build
```

Configure production owner API access:

```bash
cd platforms/cloudflare/workers/router
printf '%s' '<random-token>' | npx wrangler secret put OWNER_API_TOKEN --env production
```

The Tauri app stores its local instance URL and owner token in the platform app
configuration directory. On macOS development builds this is under:

```text
~/Library/Application Support/social.dais.owner/owner-settings.json
```

Android readiness notes:

- Keep UI behavior responsive at 360px width.
- Do not add workflows that require Wrangler, local D1 files, or Cloudflare
  account credentials in the app.
- Owner workflows should go through the scoped owner API, with revocable tokens
  stored by the platform once scoped token issuance is implemented.
- Android packaging will require the Android SDK/NDK and the Tauri Android
  target setup, but that should be packaging work, not an app rewrite.
