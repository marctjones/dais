# Implementation Honesty Audit

Use this audit before starting new roadmap feature work and before closing a
post-roadmap hardening milestone.

```bash
scripts/audit-implementation-honesty.sh
```

The script writes a Markdown report under `tmp/implementation-honesty-*/` and
fails if it finds unclassified signals such as placeholder behavior, dummy
implementations, unimplemented code paths, or shortcut language in production
code.

## Classification Rules

- **Intentional compatibility stubs** are allowed only when they are documented
  and tested. Example: the Mastodon OAuth compatibility shape returns the
  non-authenticating `owner-token-required` value and conformance tests verify
  that it does not grant access.
- **Explicit unsupported operations** must fail closed or report `UNKNOWN`.
  They must not report fake success values such as queue depth `0`.
- **Test fixtures and seed data** are allowed in tests, fixture builders,
  conformance harnesses, and seed scripts.
- **Production shortcuts** must be fixed directly when small, or converted into
  GitHub issues under the active milestone when larger.

Do not add file-based backlog entries or inline TODO comments as audit output.
Use GitHub issues for any remaining work.
