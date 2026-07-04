# Cloudflare Workers

The active deploy surface is:

- `landing` - project homepage and apex WebFinger proxy.
- `router` - ActivityPub, owner API, media, E2EE, search, Mastodon-compatible
  read surfaces, and owner-facing AT Protocol discovery/search integrations.
- `pds` - AT Protocol/PDS and Bluesky-compatible public repo/AppView endpoints
  for `pds.dais.social` and `pds.skpt.cl`.

The default local launcher mirrors that active surface:

```bash
scripts/dev-start.sh
```

The split workers below are legacy compatibility sources:

- `actor`
- `auth`
- `delivery-queue`
- `inbox`
- `outbox`
- `webfinger`

They remain in the repository for rollback, historical route configs, and
targeted migration reference. Do not treat them as the default production
surface. The default deploy path intentionally skips them:

```bash
scripts/deploy.sh deploy --env production --yes
```

Deploy or build legacy split workers only when the task explicitly requires it:

```bash
scripts/deploy.sh build --include-legacy
scripts/deploy.sh deploy --env production --include-legacy --yes
scripts/deploy.sh deploy --env production --only webfinger --yes
```

Any feature added to a legacy split worker must either also be added to `router`
or have an issue explaining why that worker is being revived.
