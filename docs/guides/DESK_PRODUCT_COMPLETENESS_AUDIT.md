# Dais Desk Product Completeness Audit

Use this audit before closing Desk owner-workflow or GUI release issues. It is a
release gate, not a backlog. Missing work found here must become GitHub issues
under epic #70 or the relevant milestone epic; do not add TODO files or inline
TODO/FIXME comments.

## Source Documents

- `docs/POSITIONING.md`: product purpose, private-by-default persona, and the
  three-mode product: public post, private post, and DM.
- `docs/design/PRIVATE_MODE.md`: private-mode behavior, authorized fetch,
  follower approval, and private graph rules.
- `docs/design/DAIS_DESK_PRODUCT_UX.md`: Dais Desk purpose, daily workflow order,
  screen model, privacy/safety rules, and visual posture.
- `docs/research/MAINSTREAM_SOCIAL_PLATFORM_LESSONS.md`: mainstream social
  expectations worth borrowing without importing engagement-feed incentives.
- `docs/guides/DESIGN_ALIGNMENT_MATRIX.md`: screen coverage and screenshot
  evidence names.

## Audit Checklist

| Area | Product-doc anchors | Required Desk evidence | Release gate |
| --- | --- | --- | --- |
| Home / Today | `DAIS_DESK_PRODUCT_UX.md` sections 3, 4, 10; `POSITIONING.md` sections 1-2 | Today, Reading, Inbox, My Posts, and Saved/Drafts rows expose reading, attention, response, and owner-only saved-state workflows. | `product_completeness_primary_workflows_are_not_placeholders` walks `today`, `reading`, `inbox`, `posts`, and `saved`; visual smoke requires `home*` screenshots. |
| Compose | `POSITIONING.md` three actions; `PRIVATE_MODE.md`; `DAIS_DESK_PRODUCT_UX.md` sections 3, 4, 9, 10 | Compose shows audience before protocol route, explains who can see the post, keeps public posting deliberate, and supports draft/media context rows. | Product completeness test walks `compose`; release visual smoke requires `home-compose-media.png`. |
| Direct Messages | `POSITIONING.md` section 1; `DAIS_DESK_PRODUCT_UX.md` Direct Messages | DMs appear in Today/Inbox, expose reply context, and show direct/encrypted state without raw recipient URLs as primary labels. | Product completeness test treats `today` and `inbox` as DM-carrying primary workflows. |
| People / Relationships | `DAIS_DESK_PRODUCT_UX.md` sections 3, 4, 9, 10 | Find, Relationship, Friends, Followers, Following, Watches/Sources, Audience Groups, and Blocks/Mutes are present with state-aware rows and next-step empty states. | Product completeness test walks all People screens; visual smoke requires `people-*` screenshots. |
| Discovery | `DAIS_DESK_PRODUCT_UX.md` Find/Search/Discovery; `MAINSTREAM_SOCIAL_PLATFORM_LESSONS.md` | Finder supports handles, URLs, sources, provider results, starter bundles, and public-search guardrails. | Product completeness test walks `find` and `watches`; release smoke captures `people-find-search` and `people-watches-sources`. |
| Server / Operations | `DAIS_DESK_PRODUCT_UX.md` sections 3, 5, 7, 10 | Health, Deliveries, Moderation, Security, Identity, Accounts, Settings, and Stats expose operator state without making raw protocol machinery primary. | Product completeness test walks all Server screens; visual smoke requires `server-*` screenshots. |
| Media | `DAIS_DESK_PRODUCT_UX.md` Compose, Thread/Post Inspector, Watches/Sources | Compose media rows show access/alt-text consequences; public media polish and encrypted media remain separately tracked roadmap work. | Product completeness test covers current compose/media rows; unresolved encrypted media must stay in GitHub issues, not placeholders. |
| Settings / Accounts | `DAIS_DESK_PRODUCT_UX.md` Global Shell, Settings, Account Switching | Defaults, route, authorized fetch, manual follower approval, and account token state are visible and actionable. | Product completeness test walks `settings` and `accounts`; visual smoke requires `server-settings` and `server-accounts`. |

## Placeholder Rules

Primary workflow screens must not ship as empty shells. A screen fails the
product-completeness gate when all of its rows are empty states or when any
primary workflow row contains placeholder language such as `not implemented`,
`coming soon`, `placeholder`, `stub`, or `TODO`.

Exceptions:

- Text input placeholder labels in `app.slint` are allowed.
- Explicitly unimplemented high-risk roadmap work, such as encrypted media, may
  remain blocked only when it is not claimed as a completed primary workflow and
  has an open GitHub issue.
- Empty-state rows are allowed when fixture data for a workflow is genuinely
  absent, but each empty state must explain the next user action and must not be
  the only evidence for a claimed primary workflow in release notes.

## Finding Workflow

When the audit finds a gap:

1. Open or update a GitHub issue with the `gui` label and the relevant milestone.
2. Link the failing screen, product-doc anchor, and release evidence.
3. Keep the release issue open until the gate passes or the risk is explicitly
   accepted in the GitHub issue.
