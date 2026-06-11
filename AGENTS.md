# Agent Instructions

dais — a single-user, **private-by-default** social server (Cloudflare; ActivityPub
+ AT Protocol). See `CLAUDE.md` for orientation and `docs/POSITIONING.md` for the
why.

## Task & knowledge tracking — GitHub issues

This project tracks tasks, decisions, and research as **GitHub issues**, grouped
under epic **#70**. Use the `gh` CLI:

- `gh issue list` / `gh issue view <n>` — see open work
- `gh issue create --title "…" --label "…"` — file a task, bug, or decision
- `gh issue comment <n> --body "…"` — record progress or a decision

Avoid file-based trackers (`TODO.md`, `NOTES.md`, `BACKLOG.md`, …) and inline
`TODO`/`FIXME` comments — open an issue instead.

## Design docs

- `docs/POSITIONING.md` — purpose, persona, three-mode product, business model
- `docs/design/PRIVATE_MODE.md` — private-by-default build plan
- `docs/design/PROTOCOL_ADAPTERS.md` — modular protocol-adapter architecture
