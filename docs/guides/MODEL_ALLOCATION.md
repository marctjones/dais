# Model Allocation for Roadmap Work

Use this guide when assigning Codex/OpenAI model tiers to dais roadmap work.
The roadmap source of truth remains the live GitHub issue and milestone state
under epic #70; this guide only decides how much model capacity to spend on each
kind of slice.

This policy was last checked against OpenAI's model and Codex model docs on
2026-06-26. Re-check those docs before changing model names or assuming pricing,
availability, or default reasoning behavior.

## Defaults

- Use `gpt-5.4-mini` with `low` reasoning for low-risk audits, issue triage,
  documentation, release notes, small UI polish, and narrow test fixes.
- Use `gpt-5.4` with `medium` reasoning for normal Rust, router, owner API,
  Desk, and protocol implementation slices.
- Use `gpt-5.5` with `high` reasoning for crypto, privacy boundaries, private
  media, key recovery, data-loss risks, large protocol refactors, release gates,
  and security review.
- Avoid `xhigh` except for one-shot architecture or security reviews where a bad
  plan would waste days or create a hard-to-unwind privacy/security problem.

For Codex CLI work, prefer:

```bash
codex -m gpt-5.4-mini
codex -m gpt-5.4
codex -m gpt-5.5
```

Set reasoning effort through the surface that launched the run when it supports
that control. Raise effort only when the extra reasoning cost is justified by
the risk of the change.

## Roadmap Matrix

| Roadmap step | Best model | Reasoning effort | Why |
| --- | --- | --- | --- |
| Roadmap audit, issue closure, evidence comments | `gpt-5.4-mini` | `low` | Mostly search, classification, and concise GitHub updates. |
| Small docs updates and release notes | `gpt-5.4-mini` | `low` | Low complexity; optimize cost and speed. |
| v1.29 E2EE DM compose/decrypt UI | `gpt-5.5` | `high` | Security-sensitive client/server/data-flow work. |
| v1.29 local key storage, rotation, recovery UX | `gpt-5.5` | `high` | Private-key handling and user-risk messaging need strong judgment. |
| v1.29 OpenMLS/MLS module design | `gpt-5.5` | `high`; `xhigh` only for final architecture review | Cryptographic protocol integration is the highest-risk area. |
| v1.29 private-mode regression gates | `gpt-5.4` | `medium` | Test architecture and release scripts need rigor, but not frontier-only. |
| v1.30 private ActivityPub media authorized fetch | `gpt-5.5` | `high` | Privacy boundary and signed-fetch behavior. |
| v1.30 encrypted media attachments | `gpt-5.5` | `high` | Cryptography plus media lifecycle. |
| v1.30 ATProto public image/blob upload | `gpt-5.4` | `medium` | Protocol work, but public-only and less privacy-sensitive. |
| v1.30 Desk media polish | `gpt-5.4-mini` first, escalate to `gpt-5.4` | `low` to `medium` | UI polish is iterative; use bigger model only for cross-screen changes. |
| v1.31 move ATProto repo/record/sync into core | `gpt-5.5` | `high` | Large refactor with protocol correctness and regression risk. |
| v1.31 ATProto posting validation | `gpt-5.4` | `medium` | Constrained protocol validation and tests. |
| v1.31 Bluesky reading/search/thread parity | `gpt-5.4` | `medium` | Multi-endpoint integration, but lower risk than crypto. |
| v1.31 Bluesky follow/social graph parity | `gpt-5.4-mini` for first pass, `gpt-5.4` for implementation | `low` then `medium` | Mostly capability labeling and public graph flows. |
| v1.32 watches/sources/public search audit | `gpt-5.4-mini` | `low` | Much of this already exists; mostly verify and close gaps. |
| v1.32 private communities/groups | `gpt-5.5` | `high` | Audience semantics intersect with E2EE and privacy. |
| v1.33 Desk owner workflow polish | `gpt-5.4-mini` for small slices; `gpt-5.4` for full-screen redesign | `low` to `medium` | Visual/UI iteration benefits from speed unless redesign is broad. |
| v1.33 GUI automation/product completeness gate | `gpt-5.4` | `medium` | Cross-platform automation and screenshots need careful test design. |
| v1.34 backup/restore/export | `gpt-5.5` | `high` | Data loss risk makes this high-stakes. |
| v1.34 managed provisioning/observability/runbooks | `gpt-5.4` | `medium` | Infra workflows need reliability, but not crypto-level reasoning. |
| Final milestone release review/tag decision | `gpt-5.5` | `high` | Use the strongest reasoning tier for final cross-cutting release confidence. |

## Operating Rules

Start each milestone with `gpt-5.4-mini` at `low` reasoning for audit and issue
classification. Use live GitHub issues and milestones, not `docs/ROADMAP.md`,
to decide current sequencing.

Implement most medium-risk Rust, router, owner API, Desk, and test slices with
`gpt-5.4` at `medium` reasoning. Escalate to `gpt-5.5` at `high` reasoning when
the slice touches E2EE, key material, authorized private reads, private media,
data recovery, large ATProto core migration, or release/security gates.

Keep one task on one tier unless the work changes risk class. If a light audit
finds a high-risk implementation gap, record the finding and relaunch or
delegate the implementation with the stronger model tier instead of continuing
the risky work on the audit model.

## Reference Links

- OpenAI API model docs: https://platform.openai.com/docs/models
- OpenAI reasoning guide: https://platform.openai.com/docs/guides/reasoning
- Codex model docs: https://developers.openai.com/codex/models
