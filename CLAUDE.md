# CLAUDE.md

dais — a self-sovereign, **private-by-default** personal social network. Single-user
server on Cloudflare (D1 / R2 / Queues / Access), federating over **ActivityPub**
and **AT Protocol (Bluesky)**.

## Orientation

- **Why & for whom:** `docs/POSITIONING.md` — purpose, persona, the three-mode
  product (post publicly · post privately to friends · DM a person).
- **Design:** `docs/design/PRIVATE_MODE.md`, `docs/design/PROTOCOL_ADAPTERS.md`,
  `docs/design/DAIS_DESK_PRODUCT_UX.md`.
- **Architecture:** `docs/ARCHITECTURE.md` (three layers: `core/` →
  `platforms/cloudflare/bindings` → worker shims). Rust core in `core/`,
  Cloudflare workers under `platforms/cloudflare/`, Rust CLI/TUI in `client/`.

## Working agreements

- **Track work in GitHub issues** (`gh issue …`), grouped under epic **#70**. Do
  not create `TODO.md` / `NOTES.md` / `SCRATCH.md` / `BACKLOG.md` style trackers,
  and avoid inline `// TODO` / `# FIXME` comments — open an issue instead.
- **Allocate model capacity by risk** using `docs/guides/MODEL_ALLOCATION.md`.
  Start with mini for audit/docs/triage, use the standard model for normal
  implementation, and reserve the strongest model for crypto, privacy
  boundaries, data recovery, large protocol refactors, release gates, and
  security review.
- **Cloudflare is the only supported deployment target** (Vercel/Netlify dropped).
- Commit/push only when asked; branch off rather than committing to `release/*`
  or `main`.
