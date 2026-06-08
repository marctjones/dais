# Design: dais Client (CLI + TUI) — Greenfield Redesign

**Status:** Proposal. A from-scratch redesign of the dais operator client, not bound
to the current Python/Click/Rich stack, UX, or command layout.
**Grounded in:** a 2026 research pass on CLI/TUI UX (clig.dev, 12-factor CLI, GitHub
CLI, Textual, Bubble Tea/Charm, Ratatui, fzf, toot/tut, aerc/neomutt). Cited inline.

---

## 0. What this client is (and isn't)

dais is **single-user-per-instance**, like a personal mail server. The client is a
**local operator tool** — no login, no multi-user. It manages *your* instance: it
reads your home timeline, composes posts, approves followers, handles DMs and
notifications, and does light admin. The mental model that fits best is therefore
**"a mail client for the fediverse"** — and the closest UX exemplars are terminal
email clients (aerc, neomutt) as much as social TUIs (toot, tut).

Two surfaces, one brain:
- **CLI** — scriptable, composable, Unix-pipe-friendly. The automation surface.
- **TUI** — an interactive reading/managing surface for daily use.

Both are thin front-ends over **one shared SDK** ("CLI-first, TUI as a presentation
layer"). This is the pattern behind the most-loved tools (gh + gh-dash, lazygit).

---

## 1. Design principles (verified, cited)

| # | Principle | Source |
|---|---|---|
| P1 | **Human-readable output is the default**; machine output is an explicit `--json` opt-in (plus `--jq`/`--template` for power users). | clig.dev, GitHub CLI |
| P2 | **Do NOT switch output format based on TTY detection** — that surprises users and breaks scripts. Use an explicit flag. *(This common belief was refuted in research.)* TTY detection is for **color only**. | clig.dev (refuted-claim) |
| P3 | **Color**: on when stdout is a TTY; honor `NO_COLOR` (any value → off) and `CLICOLOR`/`CLICOLOR_FORCE`. | GitHub CLI source |
| P4 | **Config precedence**: flags > env vars > project config > user config > defaults. | clig.dev |
| P5 | **Noun-verb command structure** (`dais timeline home`, `dais follow approve`) + **stdin piping** (`echo "hi" \| dais post`). | toot, clig.dev |
| P6 | **Async / non-blocking everywhere** — network I/O must never block the event loop, or keypresses lag. Use workers/goroutines. | Textual workers |
| P7 | **Incremental rendering** (diff the screen, never full-clear) to eliminate flicker. Modern frameworks do this for you. | Will McGugan, SE Radio 669 |
| P8 | **Command palette**: fuzzy search, shows suggestions when empty, and **isolates provider errors** so one broken command can't crash the UI. | Textual |
| P9 | **Keyboard-first, mouse optional**: Tab/Shift-Tab focus + vim `hjkl`/arrows + context-sensitive single-letter actions on the focused item. | Textual, toot |
| P10 | **Confirm outward/irreversible actions** (posting publicly, approving a follower, deleting). Approval is one keystroke but never accidental. | clig.dev |
| P11 | **Progressive disclosure**: simple by default, power on request (filters, `--json`, raw view). | GitHub CLI |
| P12 | **Performance-conscious defaults** — don't ship slow options (heavy parsing, eager fetching) in the default path. | fzf |

Anti-patterns to avoid (from research): output that changes shape when piped (P2);
blocking the UI on the network (P6); full-screen clears (P7); mystery-meat
single-letter keys with no discoverability (mitigate with the palette + a help
overlay); over-configurable keybindings as a substitute for good defaults (tut's
80+ keybindings were *not* validated as best practice).

---

## 2. Tech stack recommendation

**Primary recommendation: Go + Cobra (CLI) + Bubble Tea / Lipgloss / Bubbles / Glamour / Huh (TUI).**

Why:
- **Every tool we admire in this space is this stack** — gh, lazygit, k9s, gum, glow,
  atuin, gh-dash. It's the proven gold standard for *exactly* a loved CLI+TUI.
- **Single static binary** — `dais` is one download, no Python venv. Huge install-UX
  win for a self-host tool.
- **Bubble Tea = the Elm Architecture** (Model/Update/View, framework-owned loop) →
  clean, testable, and the "shared core, two front-ends" split falls out naturally:
  Cobra commands and Bubble Tea both call one `internal/dais` service package.
- The **Charm ecosystem** gives a cohesive, beautiful look cheaply: Lipgloss (layout/
  style), Bubbles (list/viewport/textarea/spinner), **Glamour** (render post markdown),
  **Huh** (compose forms), Gum (prompts for the CLI).

**Alternative — Rust + Ratatui + clap:** aligns with the dais *core* language, fastest,
single binary; but immediate-mode means you own the event loop (more code) and the
component ecosystem is thinner. Choose this only if "one language with the core" is a
hard requirement.

**Not recommended — staying on Python/Textual:** Textual is genuinely excellent (P6/P7/
P8 come from it), but Python distribution (venv/pipx) is a worse install story for a
download-and-run operator tool, and we're explicitly going greenfield.

> Decision needed from you (§8): **Go+Charm (recommended)** vs **Rust+Ratatui**.

---

## 3. Architecture — one SDK, two front-ends

```
            ┌──────────────┐        ┌──────────────┐
            │  cmd/dais    │        │  TUI (Bubble │
            │  (Cobra CLI) │        │  Tea program)│
            └──────┬───────┘        └──────┬───────┘
                   │   both call only ↓    │
            ┌──────────────────────────────────────┐
            │  internal/dais  — the client SDK      │
            │  • config (precedence P4)             │
            │  • signer (HTTP Signatures, your key) │
            │  • store  (local SQLite: timeline,    │
            │            unread, drafts, cache)     │
            │  • api    (Cloudflare D1 + R2 + AP)   │
            │  • e2ee   (encrypt/decrypt + fallback)│
            └──────────────────────────────────────┘
```

Key architectural upgrades over today's client:
1. **Stop shelling out to `wrangler d1 execute`.** The SDK talks to the **Cloudflare D1
   HTTP API** directly (token in config) — faster, no subprocess fragility, structured
   errors. (Longer term: a small authenticated management endpoint on the dais worker.)
2. **A real local store (embedded SQLite).** The home timeline (#63) is *ingested* (posts
   pushed to your inbox land in a local `timeline_posts` mirror) and read locally —
   instant, offline-capable, with **unread tracking** the server doesn't have to model.
3. **The SDK is the only thing that knows secrets** (your private key). CLI/TUI never
   re-implement signing or crypto — they call `signer`/`e2ee`. This is also what makes
   E2EE safe: encryption happens in the SDK, client-side, before anything leaves.

---

## 4. CLI design

### 4.1 Command map (noun-verb, P5)

```
dais
├─ timeline   home | mentions | sent | user <@h>     # read feeds
├─ post       <text|-->  [--visibility ...] [--encrypt] [--reply <id>]
├─ thread     <id>                                    # a post + its replies
├─ follow     add <@h> | list | remove <@h>           # who you follow
├─ requests   list | approve <@h> | reject <@h>       # incoming follow requests
├─ followers  list | remove <@h>
├─ friends    list                                    # mutuals (#64)
├─ dm         send <@h> <text> [--encrypt] | list | read <@h>
├─ notify     list | read [<id>|--all]
├─ block      add <@h|domain> | list | remove <@h>
├─ account    show | edit                             # your profile
├─ status                                             # instance health
└─ tui                                                # launch the TUI
```

### 4.2 Conventions
- **Human output by default** (P1). `--json` for structured; `--jq`/`--template` to
  filter inline (P11). Never auto-switch on a pipe (P2).
- **`@handle` everywhere** (`@alice@example.com`), resolved via WebFinger.
- **Pipes** (P5): `dais post -` reads stdin; `dais post --editor` opens `$EDITOR` (long-
  form, gh-style); short posts inline.
- **Confirmations** (P10): `dais post --visibility public` and `dais requests approve`
  confirm unless `--yes`. Scripts pass `--yes`.
- **Color** (P3): TTY-gated + `NO_COLOR`/`CLICOLOR`.
- **Privacy is explicit in output** — every listed/created post shows its visibility
  and encryption state (🌐 public · 👥 followers · ✉ direct · 🔒 encrypted).

Examples:
```
dais post "gm" --visibility followers           # private by default anyway
cat essay.md | dais post --editor               # compose long-form
dais requests list --json | jq '.[].handle'     # scripting
dais dm send @alice@host.com "lunch?" --encrypt # E2EE DM
```

---

## 5. TUI design — "a mail client for the fediverse"

### 5.1 The model: views + a reading pane

Borrow the email-TUI structure (aerc/neomutt): a **switchable set of views** (think
folders), a **list pane**, and a **reading/thread pane**, with **unread tracking** and
**mark-read**. Switch views with a leader key (`g`, gmail/vim style) to avoid the
classic `hjkl`-vs-sidebar keybinding clash (a research open-question — resolved here by
*not* overloading `h`/`l` for navigation).

Views: **Home · Mentions · Requests · DMs · Sent · Notifications**.

### 5.2 Main screen (home timeline)

```
┌─ dais ──────────────────────────────────────────────[ @social@dais.social ]─┐
│ Home  Mentions(3)  Requests(1)  DMs  Sent  Notifs        ⟳ 12s ago   ◉ online │
├──────────────────────────────────────────────────────────────────────────────┤
│ ● ★ Alice            @alice@coolhost.social            👥 followers   2m       │
│     Morning! Anyone else watching the launch today? Coffee in hand ☕          │
│        ↳ 4 replies   ♥ 12   ↗ 3                                                 │
│ ─────────────────────────────────────────────────────────────────────────────│
│ ○   Bob Martinez     @bob@mastodon.social              🌐 public      14m      │
│     Shipped v2 of the thing. Notes: https://…                                  │
│        ♥ 30   ↗ 8                                                               │
│ ─────────────────────────────────────────────────────────────────────────────│
│ ●   Carol            @carol@dais.carol.me               🔒 encrypted   1h       │
│     🔒 Encrypted — press ⏎ to decrypt and read in dais                          │
├──────────────────────────────────────────────────────────────────────────────┤
│ j/k move · ⏎ open · c compose · r reply · A approve · g go-to · / search · ? help│
└──────────────────────────────────────────────────────────────────────────────┘
```

Reading the UI:
- `●` unread / `○` read (P-mail model); `★` = **friend** (mutual follow, #64).
- **Visibility/encryption is always on the row** (🌐/👥/✉/🔒) — privacy is the product,
  so it's never hidden.
- An **encrypted post** shows a one-line teaser + "press ⏎ to decrypt" — decryption
  happens locally via the SDK (this is the operator's *own* dais, their key is here).
  A Mastodon user, by contrast, would have seen the fallback notice — same content,
  graceful degradation (already shipped, #71).
- **Async** (P6): the feed never blocks; a spinner/`⟳` shows background refresh; new
  posts slot in without yanking your scroll position.

### 5.3 Composer (privacy-forward)

```
┌─ Compose ─────────────────────────────────────────────────────────────────────┐
│ Replying to @alice@coolhost.social                                             │
│ ┌────────────────────────────────────────────────────────────────────────────┐ │
│ │ Same, can't wait. Did you see the new build notes?                         │ │
│ │                                                                            │ │
│ └────────────────────────────────────────────────────────────────────────────┘ │
│                                                              241 chars left     │
│ Audience:  ‹ 👥 Followers-only ›     Encrypt: [ off ]      (Tab to change)      │
│                                                                                 │
│ ⚠ Public posts federate to the whole fediverse.   (shown only when public)     │
│ 🔒 Encrypted: friends on dais read it; others see "encrypted — open in dais".   │
├──────────────────────────────────────────────────────────────────────────────┤
│ ⏎ send · ^E $EDITOR · ^V cycle audience · ^X toggle encrypt · esc cancel       │
└──────────────────────────────────────────────────────────────────────────────┘
```

- **Audience and Encrypt are first-class controls**, not buried in flags. Default is
  **Followers-only** (private-by-default, #62). Public requires a deliberate cycle +
  shows the federation warning.
- Toggling **Encrypt** previews exactly what non-dais recipients will see (the fallback
  notice) — transparency about the graceful-degradation behavior.
- `^E` drops to `$EDITOR` for long-form (gh pattern); inline for quick replies (toot
  pattern). Both, per the research open-question.

### 5.4 Requests (the approval inbox — core to private mode)

```
┌─ Follow requests ─────────────────────────────────────────────────────────────┐
│ ● Dave Park          @dave@someserver.social      asked 3h ago                 │
│   "Met you at the conf — following along!"                                     │
│   3 mutuals · account age 2y · 412 posts                  [ A approve  X reject ]│
└──────────────────────────────────────────────────────────────────────────────┘
```

Manual approval is central to the private model, so it gets a first-class view with
context (mutuals, account age) to decide. `A`/`X` act on the focused request (single
key, but confirmed for approve since it grants access to followers-only content).

### 5.5 Command palette (P8)

`Ctrl-P` (or `:`) → fuzzy, discoverable, every action reachable, errors isolated:
```
┌──────────────────────────────────────────────┐
│ > enc                                          │
│   Compose encrypted post              ^X       │
│   Compose encrypted DM…                        │
│   Toggle encryption on this draft     ^X       │
│   Settings: default encryption                 │
└──────────────────────────────────────────────┘
```
This is the discoverability backstop: nobody has to memorize keys; type what you want.

### 5.6 Keybinding scheme (resolves the hjkl conflict)

- **Move:** `j/k` (or `↓/↑`) within a list; `Ctrl-d/u` page; `g g`/`G` top/bottom.
- **Switch views (leader `g`):** `g h` Home, `g m` Mentions, `g r` Requests, `g d` DMs,
  `g n` Notifications, `g s` Sent. (Gmail/vim muscle memory; avoids `h/l` overloading.)
- **Act on focused item:** `⏎` open · `c` compose · `r` reply · `e` encrypt-reply ·
  `♥`→`f` favorite · `b` boost · `A` approve · `X` reject · `m` mark read · `y` copy link.
- **Global:** `/` search · `Ctrl-P`/`:` palette · `?` help overlay · `q` back/quit.
- Arrows + Tab always work too (P9, accessibility). A `?` overlay lists everything
  contextually so keys are discoverable, not mystery-meat.

### 5.7 Theming & accessibility
- Honor `NO_COLOR` (monochrome) and TTY (P3); ship 2–3 built-in themes (dark/light/
  high-contrast); optional `~/.config/dais/theme.toml`. Keyboard-first + Tab focus keeps
  it screen-reader-navigable (P9).

---

## 6. The reading model (ties to #63 / #64)

- **Home timeline = inbox ingestion (#63):** posts from people you follow are *pushed*
  to your inbox and mirrored into the local store; the TUI/CLI read the store (instant,
  offline, unread-tracked). This is the only way to see friends' *followers-only* posts
  (you can't pull them). Replaces today's outbox-polling timeline.
- **Friends (#64):** mutual follow = `★`; surfaced as a filter ("Home: friends only")
  and its own `dais friends` view. Optional friends-only composer audience later.
- **Unread/threading:** email-style unread + mark-read for Mentions/Requests/DMs; linear
  feed for Home with threads opening in a focused, indented thread pane.

---

## 7. Phased build plan

1. **SDK skeleton** — config (P4), Cloudflare D1/R2 API client (kill `wrangler` shelling),
   `signer`, local SQLite store. *Everything else sits on this.*
2. **CLI v1 (Cobra)** — the §4 command map over the SDK; human output + `--json`; pipes;
   confirmations. Reaches feature-parity with today's CLI on a clean base.
3. **Home timeline ingestion (#63)** — inbox stores followed posts → store; `dais timeline
   home` reads it; **outbound-follow live test** as the first checkpoint.
4. **TUI v1 (Bubble Tea)** — Home view + reading pane + composer (privacy-forward) +
   Requests + palette + help. The §5 screens.
5. **Friends (#64)** + DMs + Notifications views; encrypted-post decrypt-in-place.
6. **Polish** — themes, `?` overlays, command-palette coverage, perf pass (P12).

E2EE (#71) is already wired at the SDK layer (encrypt/decrypt + fallback), so the TUI
just surfaces it — consistent with "access protection is the default, encryption is the
opt-in."

---

## 8. Decisions needed
1. **Stack:** Go + Charm (recommended) vs Rust + Ratatui.
2. **Build vs incrementally migrate** the existing Python client (greenfield rewrite is
   cleaner given the SDK + store changes; the old client keeps working until cutover).
3. **D1 access:** direct Cloudflare D1 HTTP API now (token in config) vs a future
   authenticated management endpoint on the dais worker.
