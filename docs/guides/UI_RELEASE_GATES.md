# Dais Desk UI Release Gates

Use these gates for GUI changes that affect Dais Desk navigation, compose,
reading, discovery, moderation, settings, privacy warnings, or account switching.

## Required Command

Run the Dais Desk release gate from the repository root:

```bash
./scripts/release-desk-v2.sh
```

The script executes:

```text
SLINT_BACKEND=software DAIS_DESK_SCREENSHOT_DIR=tmp/desk-release-*/screenshots cargo test --manifest-path apps/dais-desk/Cargo.toml
cargo build --manifest-path apps/dais-desk/Cargo.toml
cargo test --manifest-path conformance/Cargo.toml -- --nocapture
```

The command runs Rust unit tests, Slint interaction tests, and the native visual
smoke test using Slint's software backend so it does not require an interactive
foreground GUI session. The visual smoke writes PNG screenshots into the release
report directory, `tmp/desk-release-*/screenshots/`.

## Required Coverage

The smoke gate must cover:

- **Home** mode: daily reading and compose-facing workflows.
- **People** mode: discovery, followers, audience groups, watches, and public
  search.
- Explicit sections: Home, Compose, Settings, Discovery, and Moderation.
- Explicit screens: home/feed, compose, inbox (including merged conversations),
  my posts with thread inspector, saved, people find/friends/follow
  requests/following, accounts, settings, and E2EE security.

Design-alignment coverage tracking lives in:

- `docs/guides/DESIGN_ALIGNMENT_MATRIX.md`
- `docs/guides/DESK_PRODUCT_COMPLETENESS_AUDIT.md`

The product-completeness gate maps Home, People, Server, Discovery, Compose,
DMs, media, and settings to the product docs. Its Rust test fails when a claimed
primary workflow screen is empty or placeholder-only.

## Accessibility Gates

The smoke gate checks and release script requirements:

- Source-list navigation uses accessible controls rather than inert text.
- Slint accessibility labels exist for the primary source-list controls and row
  cards.
- User-like automated activation can move through Home, People, Followers, and
  Compose.
- Core text colors meet contrast requirements.
- Font sizing does not depend on viewport width.
- Native screenshots for Home, People/Followers, Accounts, Settings, and
  Security are nonblank and visually varied.
- Release script enforces the required screenshot names below and still keeps
  `docs/guides/DESIGN_ALIGNMENT_MATRIX.md` as the design-coverage source.

Future GUI changes that add new icon-only controls, dialogs, sheets, or custom
interactive widgets should extend the smoke gate with targeted checks for those
controls.

## Privacy Gates

The smoke gate checks:

- Public-post warnings are present in compose logic.
- Followers-only routing preview is present.
- Direct posts require named recipients.
- Private and direct media use private media access.
- Account switching explains that all reads, posts, replies, follows, watches,
  moderation, and operator commands move to the selected instance.

Future GUI changes that alter compose, media upload, audience groups, or account
switching must add or update privacy smoke assertions before release.

## Release Issue Evidence

Every UI release issue should include:

```text
UI release gate evidence
- Branch:
- Commit:
- Command: ./scripts/release-desk-v2.sh
- Result:
- Covered modes: Home, People, settings/security surfaces
- Covered sections: Feed, Inbox (including merged conversations), Compose,
  Followers, Accounts & Tokens, Settings, Security, visual screenshots
- Accessibility notes:
- Privacy notes:
- Screenshots or video, if visual behavior changed:
```

Required screenshot names at release time:

```text
home, home-min-width, home-wide,
home-compose-media, home-compose-min-width,
home-inbox-notifications, workflow-save-post, home-today,
home-inbox-conversations, home-post-thread, home-saved,
workflow-reply-compose,
people-find-search, people-friends, people-followers, people-following,
workflow-follower-approve,
settings-accounts, settings-privacy, settings-security
```

If a gate is intentionally deferred, the release issue must name the missing
gate, explain the user risk, and link to the follow-up issue.
