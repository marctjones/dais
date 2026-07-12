# dais Roadmap

GitHub milestones and issues are the roadmap source of truth. This file is a
human-readable snapshot of the current order and the release discipline expected
for roadmap work. Update GitHub first, then update this summary when milestone
order or release policy changes.

## Current Milestone Order

No active implementation milestone is open after `v1.35`. Create or update the
next GitHub milestone before starting new product scope, then refresh this
snapshot from the live tracker.

Recently completed foundations:

- `v1.28`: independent `skpt.cl` instance deployment and cross-instance E2EE
  testbed.
- `v1.29` / `v1.29.1`: MLS/RFC 9420 v2 owner workflows, device
  publication/trust, recovery UX, legacy encryptedMessage v1 purge, and live
  dais.social <-> skpt.cl E2EE/MLS gates.
- `v1.30`: media foundations including public/private ActivityPub media,
  ATProto public image upload, and encrypted media attachment validation.
- `v1.31`: Bluesky/public protocol completion, protocol conformance gates, and
  active-router decomposition into focused modules.
- `v1.32`: RSS/website sources, private RSS/ActivityPub/Bluesky watches, public
  search, source provenance, and private community/group primitives with private
  membership by default.
- `v1.33`: Dais Desk usability and GUI quality gates.
- `v1.34`: managed hosting and operations workflows, including provisioning,
  backup/restore/export verification, migration/import tooling, health checks,
  observability, runbooks, support boundaries, and account-policy guidance.
- `v1.35`: post-roadmap hardening and product-readiness gates, including strict
  implementation-honesty and E2EE/MLS audits, expanded Desk visual regression
  coverage, fresh-environment disaster-recovery drill evidence, CI-safe gates,
  release-evidence packaging, and managed-health JSONL support.
- `v1.28.142`: current release checkpoint. The 2026-07-08 production/skpt deploy
  passed strict server tests, production/skpt builds, Bluesky and Mastodon API
  conformance, D1 update gates, production/skpt deploys, skpt live smoke, and
  cross-instance E2EE/MLS live smoke. The live `https://dais.social` homepage
  was updated after that gate.

## Immediate Priorities

1. **Keep v1.35 gates green.**
   Run the CI-safe hardening gates for routine changes and the strict
   server/Desk release gates before production, skpt, or GUI releases.

2. **Open the next milestone in GitHub before adding scope.**
   New product work should get a parent issue, focused child issues, and closure
   evidence before implementation starts.

3. **Keep docs and the homepage aligned with deploys.**
   When production or skpt changes ship, update README/project docs and the
   `https://dais.social` homepage in the same release pass.

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

Full production/skpt release gate:

```bash
scripts/release-server.sh --deploy --strict --bluesky-conformance --mastodon-conformance
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
gh issue list --state open
```
