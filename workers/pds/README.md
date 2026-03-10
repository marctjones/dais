# AT Protocol Personal Data Server (PDS)

Self-hosted AT Protocol server for `social.dais.social`. This gives you **true data ownership** while leveraging Bluesky's network infrastructure for discovery.

## Architecture

```
Your PDS (Cloudflare Workers + D1)
  ↓ stores all your posts
  ↓ serves AT Protocol API
  ↓ syncs to Bluesky Relay
  ↓ discoverable via Bluesky AppView
```

You control 100% of your data. Bluesky's network provides discovery/federation.

## Endpoints

### Identity
- `GET /.well-known/did.json` - DID document (did:web:social.dais.social)

### XRPC API
- `POST /xrpc/com.atproto.server.createSession` - Authenticate
- `GET /xrpc/com.atproto.server.getSession` - Get session info
- `POST /xrpc/com.atproto.repo.createRecord` - Create post
- `GET /xrpc/com.atproto.repo.listRecords` - List posts
- `GET /xrpc/com.atproto.sync.getRepo` - Repo export for relay
- `GET /xrpc/com.atproto.server.describeServer` - Server metadata

## Setup

### 1. Deploy Worker

```bash
cd workers/pds
wrangler deploy --env production
```

### 2. Configure DNS

Add these records to `dais.social`:

```
A     pds.dais.social  -> Cloudflare Workers IP
TXT   _atproto.social.dais.social  -> did=did:web:social.dais.social
```

### 3. Set Secrets

```bash
# Set PDS password for authentication
wrangler secret put PDS_PASSWORD --env production
# Enter a strong password when prompted
```

### 4. Update CLI

The CLI will automatically use your self-hosted PDS instead of posting to Bluesky's API.

## Data Storage

All AT Protocol records are stored in your D1 database:
- Posts: `posts` table with `protocol='atproto'` or `protocol='both'`
- Each post has `atproto_uri` and `atproto_cid` fields

## Authentication

Uses simple password auth (stored in secrets). The PDS generates session tokens for authenticated requests.

**Credentials:**
- Handle: `social.dais.social`
- Password: Set via `wrangler secret put PDS_PASSWORD`

## Sync with Bluesky Network

The PDS implements `com.atproto.sync.getRepo` which allows Bluesky's relay to sync your posts. This makes you discoverable across the AT Protocol network while maintaining full data ownership.

## Comparison

| Aspect | Your Setup | Using bsky.social |
|--------|------------|-------------------|
| Data ownership | ✅ Yours (D1) | ❌ Theirs |
| Discovery | ✅ Via Bluesky relay | ✅ Native |
| Control | ✅ Full | ❌ Limited |
| Complexity | Medium | Low |
