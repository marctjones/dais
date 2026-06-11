# dais Owner Tauri App

The first-party owner app lives in `apps/owner-tauri`. It is a Tauri v2 desktop
shell intended to share Rust owner-client models with the CLI/TUI through
`client-core`, and to become Android-capable later without a UI rewrite.

Current status:

- Adaptive desktop/narrow layout for Home, Posts, Sources, Notifications,
  Followers, Profile, Moderation, Deliveries, Settings, and Diagnostics.
- Local settings storage for instance URL and owner token.
- Shared Rust `dais-client-core` models for privacy badges, protocol warnings,
  source items, moderation state, diagnostics, and snapshots.
- Placeholder data until the scoped HTTPS owner API is implemented.

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

Android readiness notes:

- Keep UI behavior responsive at 360px width.
- Do not add workflows that require Wrangler, local D1 files, or Cloudflare
  account credentials in the app.
- Owner workflows should go through the future scoped owner API, with revocable
  tokens stored by the platform.
- Android packaging will require the Android SDK/NDK and the Tauri Android
  target setup, but that should be packaging work, not an app rewrite.
