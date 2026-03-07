# Actor Worker

ActivityPub Actor endpoint for dais.social.

## Endpoint

`GET https://social.dais.social/users/{username}`

## Response

Returns an ActivityPub Person object:

```json
{
  "@context": [
    "https://www.w3.org/ns/activitystreams",
    "https://w3id.org/security/v1"
  ],
  "type": "Person",
  "id": "https://social.dais.social/users/marc",
  "preferredUsername": "marc",
  "name": "Marc",
  "summary": "Building my own corner of the fediverse",
  "inbox": "https://social.dais.social/users/marc/inbox",
  "outbox": "https://social.dais.social/users/marc/outbox",
  "followers": "https://social.dais.social/users/marc/followers",
  "following": "https://social.dais.social/users/marc/following",
  "publicKey": {
    "id": "https://social.dais.social/users/marc#main-key",
    "owner": "https://social.dais.social/users/marc",
    "publicKeyPem": "-----BEGIN PUBLIC KEY-----\n..."
  },
  "manuallyApprovesFollowers": true,
  "url": "https://dais.social/@marc"
}
```

## Development

```bash
# Build and run locally
wrangler dev

# Test
curl -H "Accept: application/activity+json" http://localhost:8787/users/marc

# Deploy
wrangler deploy
```

## Database

Queries the `actors` table in D1:
- username
- display_name
- summary
- public_key

Actor must be seeded into database before deployment.
