# Phase 1 Deployment Guide

## Prerequisites

1. **Cloudflare Account**: Ensure you're logged in via `wrangler`
2. **Custom Domains**: DNS must be configured in Cloudflare:
   - `dais.social` → WebFinger worker
   - `social.dais.social` → Actor and Inbox workers

## DNS Configuration

In your Cloudflare dashboard, add the following DNS records:

```
Type: CNAME
Name: dais.social
Target: <your-webfinger-worker>.workers.dev
Proxied: Yes (orange cloud)

Type: CNAME
Name: social
Target: <your-actor-worker>.workers.dev
Proxied: Yes (orange cloud)
```

## Deploy Workers

### 1. Deploy WebFinger Worker

```bash
cd workers/webfinger
wrangler deploy --env production
```

Expected endpoints:
- `https://dais.social/.well-known/webfinger?resource=acct:marc@dais.social`

### 2. Deploy Actor Worker

```bash
cd workers/actor
wrangler deploy --env production
```

Expected endpoints:
- `https://social.dais.social/users/marc`

### 3. Deploy Inbox Worker

```bash
cd workers/inbox
wrangler deploy --env production
```

Expected endpoints:
- `https://social.dais.social/users/marc/inbox`

## Verify Deployment

### Test WebFinger
```bash
curl -H "Accept: application/jrd+json" \
  "https://dais.social/.well-known/webfinger?resource=acct:marc@dais.social"
```

Expected response:
```json
{
  "subject": "acct:marc@dais.social",
  "links": [{
    "rel": "self",
    "type": "application/activity+json",
    "href": "https://social.dais.social/users/marc"
  }]
}
```

### Test Actor
```bash
curl -H "Accept: application/activity+json" \
  "https://social.dais.social/users/marc"
```

Expected response: ActivityPub Person object with public key

## Post-Deployment

1. **Test Federation**: Try following `@marc@dais.social` from a Mastodon instance
2. **Monitor Logs**: Use `wrangler tail` to watch incoming requests
3. **Approve Followers**: Use the CLI to manage follow requests

```bash
# List pending followers
dais followers list --status pending --remote

# Approve a follower
dais followers approve <actor-url> --remote
```

## Troubleshooting

### Worker not accessible via custom domain
- Check DNS propagation: `dig dais.social`
- Verify routes in `wrangler.toml`
- Ensure custom domain is added in Cloudflare Workers dashboard

### HTTP 404 on endpoints
- Check worker logs: `wrangler tail <worker-name> --env production`
- Verify routing patterns match request paths

### Follow requests not working
- Check inbox worker logs for signature verification
- Ensure public key is present in actor endpoint
- Verify D1 database is accessible
