# Inbox Worker

ActivityPub Inbox endpoint for receiving activities.

## Endpoint

`POST https://social.dais.social/users/{username}/inbox`

## Supported Activities

### Follow
Receives follow requests from other ActivityPub actors.
- Stores in D1 with status='pending'
- Requires manual approval via CLI: `dais followers approve @user@domain`

### Undo
Handles unfollows (Undo Follow activities).
- Removes follower from database

### Accept/Reject (Phase 3)
Responses to our follow requests when we follow others.

## HTTP Signatures

All incoming requests must include a valid HTTP Signature header:
```
Signature: keyId="https://example.com/users/alice#main-key",algorithm="rsa-sha256",headers="(request-target) host date",signature="..."
```

**Current Status:** Signature validation is logged but not enforced (TODO)

## Database

Stores activities in the `followers` table:
- id: Activity ID
- follower_actor_id: Actor URL (e.g., https://mastodon.social/users/alice)
- follower_inbox: Inbox URL for sending responses
- status: 'pending', 'approved', or 'rejected'

## Development

```bash
# Build and run locally
wrangler dev

# Test
curl -X POST -H "Content-Type: application/activity+json" \
  -d '{"type":"Follow","id":"...","actor":"...","object":"..."}' \
  http://localhost:8787/users/marc/inbox

# Deploy
wrangler deploy
```

## Next Steps

1. Implement actor public key fetching
2. Enforce HTTP signature verification
3. Add caching for actor public keys
4. Support more activity types (Create, Like, Announce)
