# Dais Desk UI Release Gates

Use these gates for GUI changes that affect Dais Desk navigation, compose,
reading, discovery, moderation, settings, privacy warnings, or account switching.

## Required Command

Run the owner app smoke from the Tauri app directory:

```bash
cd apps/owner-tauri
npm run smoke
```

The command builds the Vite bundle and runs `scripts/owner-tauri-smoke.mjs`.

## Required Coverage

The smoke gate must cover:

- **Home** mode: daily reading and compose-facing workflows.
- **People** mode: discovery, followers, audience groups, watches, and public
  search.
- **Server** mode: settings, diagnostics, moderation, profile, and operator
  state.
- Explicit sections: Home, Compose, Settings, Discovery, and Moderation.

## Accessibility Gates

The smoke gate checks:

- Keyboard-operable navigation uses controls rather than inert text.
- Visible focus styling exists.
- Screen-reader labeling exists for the account switcher.
- Core text colors meet contrast requirements.
- Font sizing does not depend on viewport width.
- Dark-mode and narrow-layout CSS are present.

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
- Command: cd apps/owner-tauri && npm run smoke
- Result:
- Covered modes: Home, People, Server
- Covered sections: Home, Compose, Settings, Discovery, Moderation
- Accessibility notes:
- Privacy notes:
- Screenshots or video, if visual behavior changed:
```

If a gate is intentionally deferred, the release issue must name the missing
gate, explain the user risk, and link to the follow-up issue.
