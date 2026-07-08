# dais Roadmap

GitHub milestones and issues are the roadmap source of truth. This file is a
human-readable snapshot of the current order and the release discipline expected
for roadmap work. Update GitHub first, then update this summary when milestone
order or release policy changes.

## Current Milestone Order

| Order | Milestone | Focus | Open work |
| --- | --- | --- | --- |
| 1 | `v1.35 - Post-roadmap hardening and product readiness` | Quality hardening after the completed v1.28-v1.34 roadmap: implementation honesty audit, Desk GUI regression confidence, E2EE/MLS security review, disaster recovery, CI/release evidence, and managed-ops polish. | #349, #356, #353, #355, #354, #351, #350 |

Recently completed foundations:

- `v1.28`: independent `skpt.cl` instance deployment and cross-instance E2EE
  testbed.
- `v1.29` / `v1.29.1`: encryptedMessage v1 fallback, OpenMLS/MLS v2 owner
  workflows, device publication/trust, recovery UX, and live dais.social <->
  skpt.cl E2EE/MLS gates.
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
- `v1.28.142`: current release checkpoint. The 2026-07-08 production/skpt deploy
  passed strict server tests, production/skpt builds, Bluesky and Mastodon API
  conformance, D1 update gates, production/skpt deploys, skpt live smoke, and
  cross-instance E2EE/MLS live smoke. The live `https://dais.social` homepage
  was updated after that gate.

## Immediate Priorities

1. **Start with the implementation honesty audit (#356).**
   Before adding new feature scope, audit server, core, client, Desk, scripts,
   docs, and conformance code for placeholders, dummy behavior, shortcuts,
   compatibility stubs, and unimplemented APIs. Fix small clear problems
   directly; file focused follow-ups for larger findings.

2. **Raise confidence in the user-facing and private paths (#353, #355).**
   Harden Dais Desk automated visual/interaction gates without depending on
   manual focus-taking runs, and run a dedicated E2EE/MLS security review now
   that live cross-instance encrypted-message gates pass.

3. **Prove operations are repeatable (#354, #351, #350).**
   Run a fresh-environment backup/restore disaster-recovery drill, automate more
   CI/release evidence, and polish production observability plus managed-instance
   support workflows.

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
gh issue list --milestone "v1.35 - Post-roadmap hardening and product readiness"
```
