# Managed Hosting Plan

This guide defines the managed `dais.cloud` direction. It is product policy for
future managed hosting, not a claim that a public managed service is live.

Managed Dais must make private-by-default social hosting easier without changing
the product bargain: the owner controls the graph, private content stays private,
and revenue comes from hosting, portability, backup, support, and convenience.

## Tier Model

| Tier | Intended owner | Included baseline | Paid differentiators |
| --- | --- | --- | --- |
| Self-hosted OSS | Technical owner | Source code, documented Cloudflare deploys, backup/export tools | None in product; optional paid support can exist outside the software |
| Personal managed | One person with one primary actor | Custom or managed subdomain, private-by-default ActivityPub, public Bluesky-compatible surface, E2EE, backups, export | Storage, media bandwidth, support response, custom domain assistance |
| Family managed | Household or close family circle | Personal baseline plus private groups, admin consent rules, safer defaults for minors | Additional seats, extra backup retention, onboarding help |
| Creator managed | Person or small publication with public posts | Personal baseline plus public posting, media, migration/import help | Storage/bandwidth, custom domain, paid migration, stronger observability |
| Community/org managed | Club, school group, research group, small business, or professional association | Group or Organization actor, member lists, moderation logs, exports, admin roles | Seats, support SLA, compliance/export assistance, domain migration |
| Support-only | Existing self-hosted instance | Operational review, restore rehearsal, migration assistance | Time-bound support and incident response |

Privacy, safety, export, account deletion, E2EE, and basic backups are baseline
trust features. They must not be paywalled. Paid tiers can increase retention,
storage, support, domains, and convenience, but cannot make private posts safer
only for paying users.

## Monetization Rules

Dais managed hosting rejects:

- behavioral advertising;
- sale or rental of social graphs;
- training on private content;
- engagement-ranked feeds optimized for time-on-site;
- dark-pattern retention, hidden cancellation, or export friction;
- operator inspection of private content except through explicit owner-granted
  recovery/support artifacts.

Revenue can come from:

- managed Cloudflare hosting;
- storage, media bandwidth, and backup retention;
- custom domain setup;
- support and restore rehearsal;
- import/migration assistance;
- optional creator/community services that do not alter private defaults.

The closest operating models are managed email/domain hosts, managed open-source
hosting such as WordPress.com, Ghost(Pro), and Discourse hosting, and Mastodon
hosting/support providers. The useful pattern is subscription for reliable
operations, not monetization of attention.

## Account And Actor Model

Dais remains single-owner by default. Managed hosting can expose different actor
types without becoming a general multi-tenant platform:

- **Person:** one owner controls one personal actor. This is the default.
- **Family group:** a managed `Group` actor for a household or family circle.
  The accountable adult owner controls billing, domains, recovery, and member
  invitations.
- **Community group:** a `Group` actor for a club or small community. At least
  one named owner/operator is responsible for moderation and export.
- **Organization:** an `Organization` actor for a business, publication,
  professional association, or research group. The organization owns the domain
  and data-export process.

Separate contexts should use separate actors or instances. A work organization
must not silently share identity, private groups, or keys with a personal family
actor.

## Admin Visibility And Consent

Managed operators can see operational metadata needed to run the service:

- domains, routes, worker names, queue status, backup status, storage usage;
- owner account contact and billing metadata;
- aggregate counts for posts, followers, delivery failures, and media objects;
- explicit support artifacts the owner chooses to provide.

Managed operators must not read private post bodies, encrypted messages, private
media, or E2EE keys as a default support workflow. Restore and debugging flows
must separate:

- **operator metadata:** safe for managed support;
- **owner secret material:** owner-controlled, encrypted at rest, used only when
  the owner supplies the recovery secret;
- **private content:** restored or exported for the owner, not inspected by the
  operator.

Support runbooks should prefer owner-run diagnostics and redacted bundles before
requesting privileged access.

## Family And Minor Safety Defaults

Family hosting is allowed only if the product defaults stay conservative:

- private-by-default posting and approved followers;
- no public discovery of minors by default;
- member invitations require an accountable adult owner;
- private group membership is private unless the owner explicitly chooses a
  public membership surface;
- export and deletion requests are visible to the accountable owner;
- moderation and blocking tools are available on every tier.

Dais should not market directly to children until age, consent, data-retention,
and support escalation policies are explicit. Until then, family hosting means an
adult-owned private family space.

## Baseline Managed Instance Requirements

A managed instance is not ready until these operational gates pass:

- custom domain routes for web, ActivityPub, and PDS surfaces;
- isolated D1, R2, queues, worker names, and secrets per owner/instance;
- owner token configured and tested without exposing it in logs or issues;
- WebFinger, ActivityPub actor, owner API, and Desk connection smoke tests;
- backup archive and restore verification;
- live delivery/E2EE/MLS smoke when the instance participates in private
  federation tests;
- documented support boundaries and incident runbooks.

The independent `skpt.cl` instance is the proving ground for these gates. It is
not itself a managed-hosting product.

## Operational Workflows

Create a dry-run provisioning plan:

```bash
scripts/provision-managed-instance.sh \
  --slug example-family \
  --domain example.com \
  --owner-token-file /secure/path/owner-token \
  --delivery-admin-token-file /secure/path/delivery-admin-token
```

The provisioning plan writes a manifest, wrangler environment snippets for the
active router/PDS/landing/delivery workers, and the ordered Cloudflare commands
for D1, R2, Queues, secrets, deploy, and validation. Use `--apply-resources`
only after reviewing the generated snippets; apply mode creates the Cloudflare
D1/R2/Queue resources and sets provided secrets, but it still leaves worker
config review and deploy explicit.

Smoke a provisioned instance:

```bash
scripts/smoke-managed-instance.sh \
  --domain example.com \
  --activitypub-domain social.example.com \
  --pds-domain pds.example.com \
  --owner-token-file /secure/path/owner-token
```

Run managed health:

```bash
scripts/managed-health-check.sh \
  --domain example.com \
  --activitypub-domain social.example.com \
  --pds-domain pds.example.com \
  --owner-token-file /secure/path/owner-token \
  --r2-bucket dais-media-example-family
```

Verify backup restore:

```bash
scripts/verify-backup-restore.sh --self-test
DAIS_BACKUP_PASSPHRASE_FILE=/secure/path/backup-passphrase \
  scripts/verify-backup-restore.sh ~/.dais/backups/dais_production_backup_YYYYMMDDTHHMMSSZ.tar.gz.gpg
```

Preview imports before applying them:

```bash
scripts/import-preview.rb --format opml --file subscriptions.opml --output import-plan.json
scripts/import-preview.rb --format mastodon-following-csv --file following_accounts.csv
scripts/import-preview.rb --format bluesky-follows --file bluesky-handles.json
```

## Implementation Issues

The managed service needs these implementation tracks:

- repeatable provisioning workflow with dry-run and recoverable apply steps;
- fresh-environment backup restore verification;
- import preview/apply tools for Mastodon, Bluesky, RSS/OPML, and local archives;
- managed health checks that report unavailable queue depth honestly;
- support runbooks for federation, media, domain, delivery, auth, and restore
  incidents;
- Desk Server mode health display backed by owner API diagnostics.
