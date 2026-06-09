# Federation testing — driving real Mastodon + Bluesky from the command line

**Status:** notes / how-to. Complements issue **#68** (local federation smoke test).

Unit and mock-`HttpProvider` tests cover activity construction, signing, and the
client's logic, but they can't prove **real interoperability**. The wire path
(client → signed delivery → a real remote inbox, and remote → dais inbox) needs a
live counterpart. These are the CLI tools we use to script that, on both networks.

## Mastodon (ActivityPub) — `toot`

[`toot`](https://toot.readthedocs.io) is a scriptable Mastodon CLI. Point it at a
**test Mastodon account** (e.g. on `mastodon.social`) and drive the other side of
each dais interaction:

```bash
pip install toot
toot login                              # one-time, interactive
toot whoami

# 1. Inbound Follow → dais approval path:
toot follow @social@dais.social         # → dais `requests list` should show it
#   then on dais:  dais requests approve @<you>@mastodon.social   (delivers Accept)
toot whoami                             # confirms you now follow dais

# 2. Outbound post delivery:
#   on dais:  dais post "hello fedi" --visibility public
toot timeline                           # the dais post should appear

# 3. Inbound reply / mention:
toot post "@social@dais.social hi"      # → lands in dais (mentions / inbox)
```

`toot` exits non-zero on failure and emits JSON with `--json`, so it scripts cleanly
into a smoke-test harness.

## Bluesky (AT Protocol) — `goat` or the `atproto` SDK

There is no `toot` equivalent, but the AT Protocol has CLI/SDK options:

- **[`goat`](https://github.com/bluesky-social/indigo)** — the official Go AT-proto
  CLI (`go install github.com/bluesky-social/indigo/cmd/goat@latest`). Good for
  scripting posts, follows, and reads against a test Bluesky account:
  ```bash
  goat account login -u <handle>.bsky.social
  goat post "hello atmosphere"
  goat follow <did-or-handle>
  goat feed get-author <handle>          # read back
  ```
- **Python [`atproto`](https://atproto.blue) SDK** — same operations in-process; the
  retired Python CLI used it, so existing record-shapes are a reference.

## Smoke-test flow (the #68 checklist)

Run against a **staging** dais instance where possible; don't spam production peers.

1. **Post visibility:** dais posts public + followers-only; confirm a Mastodon
   follower sees the public one (and the encrypted-post fallback notice for E2EE).
2. **Approval inbox:** Mastodon follows dais → `dais requests list` → `approve` →
   `toot whoami` confirms the follow stuck (Accept delivered).
3. **DM:** `dais dm send @<test>@mastodon.social "ping"` → arrives as a direct toot.
4. **Bluesky parity (after the AT-proto port, see the parity issue):** mirror 1–3
   with `goat` on a test Bluesky account.

## Automation

A future `scripts/federation-smoke.sh` can chain `toot`/`goat` against a staging
instance and assert on their JSON output — the durable version of the manual flow
above. Requires test accounts on both networks (kept out of the repo).
