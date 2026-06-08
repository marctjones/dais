# CLAUDE.md

dais — a self-sovereign, **private-by-default** personal social network. Single-user
server on Cloudflare (D1 / R2 / Queues / Access), federating over **ActivityPub**
and **AT Protocol (Bluesky)**.

## Orientation

- **Why & for whom:** `docs/POSITIONING.md` — purpose, persona, the three-mode
  product (post publicly · post privately to friends · DM a person).
- **Design:** `docs/design/PRIVATE_MODE.md`, `docs/design/PROTOCOL_ADAPTERS.md`.
- **Architecture:** `ARCHITECTURE_v1.1.md` (three layers: `core/` →
  `platforms/cloudflare/bindings` → worker shims). Rust core in `core/`,
  Cloudflare workers under `platforms/cloudflare/`, Python CLI in `cli/`.

## Working agreements

- **Track work in GitHub issues** (`gh issue …`), grouped under epic **#70**. Do
  not create `TODO.md` / `NOTES.md` / `SCRATCH.md` / `BACKLOG.md` style trackers,
  and avoid inline `// TODO` / `# FIXME` comments — open an issue instead.
- **Cloudflare is the only supported deployment target** (Vercel/Netlify dropped).
- Commit/push only when asked; branch off rather than committing to `release/*`
  or `main`.

> IdlerGear was retired in commit 92ab1c3. Any lingering `idlergear` hooks or MCP
> config are dead — ignore them.
