# dais Roadmap

GitHub milestones and issues are the roadmap source of truth. This file is a
human-readable snapshot of the current order and the release discipline expected
for roadmap work. Update GitHub first, then update this summary when milestone
order or release policy changes.

## Current Milestone Order

| Order | Milestone | Focus | Open work |
| --- | --- | --- | --- |
| 4 | `v1.31 - Bluesky and public protocol completion` | First-class public ATProto/Bluesky surface, core repo/record/sync migration, public posting/reading/search/follow parity, server architecture cleanup, and protocol release gates. | #274, #275, #276, #277, #278, #334 |
| 5 | `v1.32 - Discovery, watches, sources, and communities` | Intentional discovery and reading: RSS/website watches, ActivityPub/Bluesky watches, public search, source provenance, and private community/group primitives. | #280, #281, #282, #283, #286 |
| 7 | `v1.34 - Managed hosting and operations` | Productized managed hosting and operations: provisioning, backups, migration/import, observability, runbooks, support, and account policy. | #294, #295, #296, #297, #298, #299, #300 |

Recently completed foundations:

- `v1.29` / `v1.29.1`: encryptedMessage v1 fallback, OpenMLS/MLS v2 owner
  workflows, device publication/trust, recovery UX, and live dais.social <->
  skpt.cl E2EE/MLS gates.
- `v1.30`: media foundations including public/private ActivityPub media,
  ATProto public image upload, and encrypted media attachment validation.
- `v1.33`: Desk owner workflow polish and release-gate foundation.
- `v1.31` cleanup: server release gate matrix (#335), retired unfinished
  Bluesky reply sidecar (#332), and clarified Mastodon OAuth compatibility-only
  auth behavior (#333).
- `v1.32` cleanup: JSON API source ingestion/status language (#331).
- `v1.34` progress: verifiable backup archive format and production/skpt
  encrypted backup smoke (#297 remains open for fresh-environment restore).

## Immediate Priorities

1. **Finish v1.31 protocol correctness.**
   Start with #275 because moving ATProto repo, record, and sync operations into
   core is the main architecture dependency for the rest of the Bluesky work.
   Track router decomposition in #334 so the active router does not absorb more
   protocol code while v1.31 expands.

2. **Continue v1.32 reader/discovery work.**
   #281, #282, and #283 should be implemented with source provenance and
   explicit public-search guardrails. #286 is higher risk because private groups
   intersect with audience semantics, delivery, and E2EE.

3. **Complete v1.34 restore/provisioning gates.**
   #297 now has a verifiable backup archive and production/skpt backup smoke,
   but still needs a fresh-environment restore test before closure. #296 and
   #299 should build on that evidence instead of adding separate ad hoc ops
   scripts.

## Coverage Policy

Every issue that changes server, protocol, privacy, media, or release behavior
must state the evidence needed to close it. The default evidence is:

- Unit or integration tests for changed Rust behavior.
- Router tests when request/response shaping or owner API behavior changes.
- Core tests when shared ActivityPub, ATProto, SQL, policy, E2EE, or MLS logic
  changes.
- Conformance tests when protocol compatibility claims change.
- Live smoke tests when deployed behavior, federation, E2EE/MLS, private media,
  or independent-instance behavior changes.
- Documentation updates when user-facing behavior, release claims, or operating
  procedures change.

Minimum server release gate:

```bash
scripts/release-server.sh --strict
```

The script records pass/fail evidence under `tmp/server-release-*/` and runs:

- `cargo test --manifest-path core/Cargo.toml`
- `cargo test --manifest-path platforms/cloudflare/workers/router/Cargo.toml`
- `cargo test --manifest-path platforms/cloudflare/bindings/Cargo.toml`
- `scripts/deploy.sh build --env production`
- `scripts/deploy.sh build --env skpt`
- `scripts/smoke-skpt-instance.sh`
- `scripts/smoke-cross-instance-e2ee.sh`
- `scripts/smoke-cross-instance-mls.sh`

Run conformance gates when protocol compatibility changes:

```bash
scripts/release-server.sh --strict --conformance
scripts/release-server.sh --strict --bluesky-conformance
scripts/release-server.sh --strict --mastodon-conformance
```

For release-critical privacy, protocol, or E2EE changes, live gates should fail
closed when prerequisites are missing. Use strict environment settings such as
`REQUIRE_FULL=1` where supported rather than accepting a silent skip.

## Issue Hygiene

- Keep new roadmap work under epic #70 and the appropriate milestone.
- Use parent epic issues for broad work and focused child issues for shippable
  slices.
- Do not use file-based backlogs or inline TODOs for roadmap work.
- When closing an issue, comment with the commit, tests run, live smoke evidence
  if applicable, and any residual risk.
- If a release gate is intentionally skipped, name the missing prerequisite,
  explain the risk, and link the follow-up issue before tagging.

## Tracker Commands

```bash
gh api repos/marctjones/dais/milestones --paginate
gh issue list --milestone "v1.31 - Bluesky and public protocol completion"
gh issue list --milestone "v1.32 - Discovery, watches, sources, and communities"
gh issue list --milestone "v1.34 - Managed hosting and operations"
```
