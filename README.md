# dais - Single-User ActivityPub Server

A generic single-user ActivityPub server that enables complete ownership of social media presence, independent of platforms. Built on Cloudflare's free tier, federating with Mastodon, Pleroma, and other ActivityPub-compatible networks.

**Example deployment**: The dais project itself uses dais at `@social@dais.social` for project updates (dogfooding).

**Deploy your own**: Works for any domain - personal (`@yourname@yourdomain.com`) or business (`@social@yourbusiness.com`).

## Features

- **WebFinger Discovery**: Discover your identity from any Fediverse client
- **ActivityPub Federation**: Full compatibility with Mastodon, Pleroma, and other instances
- **HTTP Signatures**: Cryptographically signed requests for federation trust
- **Follower Management**: Approve/reject follow requests via CLI
- **Post Publishing**: Create and publish posts to your followers
- **Media Support**: Upload and serve media attachments via Cloudflare R2
- **Post Visibility**: Public, unlisted, followers-only, and direct message support
- **Zero Cost**: Runs entirely on Cloudflare's free tier

## Architecture

### Tech Stack

- **Rust Workers (WASM)**: WebFinger, Actor, Inbox, Outbox endpoints
- **Python CLI**: Post creation, follower management, testing utilities
- **Cloudflare Infrastructure**:
  - **Workers**: HTTP request handling (Rust compiled to WASM)
  - **D1 (SQLite)**: Relational data (followers, posts, activities)
  - **R2**: Media storage (images, videos)
  - **KV**: Caching and session data
  - **Pages**: Static landing page

### Project Structure

```
dais/
├── workers/          # Cloudflare Workers (Rust → WASM)
│   ├── webfinger/   # WebFinger endpoint
│   ├── actor/       # ActivityPub Actor endpoint
│   ├── inbox/       # Receive activities
│   ├── outbox/      # Serve posts
│   └── shared/      # Shared types and utilities
├── cli/             # Python CLI for management
│   └── dais_cli/    # Post creation, follower approval, testing
└── web/             # Static landing page
```

## Quick Start

### Two Ways to Get Started

**🐳 Containerized (Recommended for Testing):**
- No local dependencies except Docker/Podman
- Clean, isolated, reproducible environment
- See [CONTAINER_QUICKSTART.md](CONTAINER_QUICKSTART.md)

```bash
make up && make seed && make test
```

**⚡ Native (For Development):**
- Fast iteration with hot reload
- Better for active development
- See full setup below

### Prerequisites (Native Setup)

- [Rust](https://rustup.rs/) (for Workers)
- [wrangler](https://developers.cloudflare.com/workers/wrangler/) (Cloudflare CLI)
- [Python 3.10+](https://www.python.org/) (for CLI)
- Cloudflare account (free tier)

### Native Setup

### 1. Install Python CLI

```bash
cd cli
pip install -e .
```

### 2. Initialize Configuration

```bash
# Generate RSA keys and create config
dais setup init

# View configuration
dais setup show
```

This creates `~/.dais/` with:
- `config.toml` - Server configuration
- `keys/private.pem` - RSA private key (4096-bit)
- `keys/public.pem` - RSA public key

### 3. Deploy WebFinger Worker

```bash
cd workers/webfinger

# Install worker-build (first time only)
cargo install worker-build

# Deploy to Cloudflare
wrangler deploy
```

### 4. Test WebFinger

```bash
# Test the endpoint
dais test webfinger

# Or manually
curl "https://dais.social/.well-known/webfinger?resource=acct:social@dais.social"
```

Expected response:
```json
{
  "subject": "acct:social@dais.social",
  "aliases": ["https://social.dais.social/users/social"],
  "links": [
    {
      "rel": "self",
      "type": "application/activity+json",
      "href": "https://social.dais.social/users/social"
    }
  ]
}
```

## Development

### Building Rust Workers

```bash
# WebFinger worker
cd workers/webfinger
wrangler dev           # Local development
wrangler deploy        # Deploy to production

# Build all workers
cd workers/webfinger && wrangler deploy
cd workers/actor && wrangler deploy
cd workers/inbox && wrangler deploy
cd workers/outbox && wrangler deploy
```

### Python CLI Development

```bash
cd cli

# Install with dev dependencies
pip install -e ".[dev]"

# Run tests
pytest

# Format code
black .
ruff check .
```

## CLI Usage

### Setup

```bash
dais setup init              # Initialize config and generate keys
dais setup show              # Show current configuration
```

### Posts

```bash
dais post create "Hello, Fediverse!"
dais post create "Unlisted post" --visibility unlisted
dais post list
dais post delete <post-id>
```

### Followers

```bash
dais followers list
dais followers approve @alice@mastodon.social
dais followers reject @bob@pleroma.example
dais followers remove @charlie@pixelfed.social
```

### Testing

```bash
dais test webfinger                    # Test WebFinger endpoint
dais test actor                        # Test Actor endpoint
dais test federation @user@domain.com  # Test federation with another instance
```

### Statistics

```bash
dais stats                             # Show follower count, post count, etc.
```

## Implementation Roadmap

### Phase 1 - Basic Federation ✅ COMPLETE

**What you can do now:**
- Be discovered on the Fediverse via WebFinger (`@username@yourdomain.com`)
- Have a valid ActivityPub profile accessible to any federated server
- Receive follow requests from Mastodon, Pleroma, Pixelfed, etc.
- Approve or reject followers via CLI
- Send cryptographically signed Accept/Reject activities

**Implementation:**
- [x] WebFinger endpoint (Rust Worker) - with D1 actor lookup
- [x] Python CLI skeleton - full command structure
- [x] RSA key generation (`dais setup init`)
- [x] Actor endpoint with static profile - queries D1 for actor data
- [x] Inbox for receiving Follow requests - with HTTP signature verification
- [x] Follower approval CLI (`dais followers approve`) - sends signed Accept/Reject
- [x] HTTP signature verification - PKCS1v15 (rsa-sha256) per ActivityPub spec
- [x] Comprehensive test coverage - pytest for CLI, cargo test for workers

**Testing:**
```bash
# Run CLI tests
cd cli && pytest -v

# Run Rust tests
cd workers/shared && cargo test

# Test WebFinger discovery
dais test webfinger

# Test Actor endpoint
dais test actor

# View statistics
dais stats
```

### Phase 2 - Content Publishing ✅ COMPLETE

**What you can do now:**
- Create and publish posts to your followers (`dais post create`)
- List all your posts (`dais post list`)
- Delete posts with federated Delete activities (`dais post delete`)
- Control post visibility (public, unlisted, followers, direct)
- Automatic delivery to all approved followers' inboxes
- Serve posts via ActivityPub-compliant outbox endpoint

**Implementation:**
- [x] Outbox worker with OrderedCollection endpoint
- [x] Individual post endpoint (GET /users/:username/posts/:id)
- [x] Post create command with HTTP signature delivery
- [x] Post list command with rich table display
- [x] Post delete command with Delete activity delivery
- [x] Activity delivery module with reusable HTTP signature logic
- [x] Post visibility controls (public, unlisted, followers, direct)
- [x] Local development environment with tmux scripts
- [x] Database seeding scripts for testing
- [x] Integration tests for both Phase 1 and Phase 2
- [x] Unit tests (19 tests covering delivery, posts, outbox)

**Testing:**
```bash
# Local development
./scripts/dev-start.sh
./scripts/seed-local-db.sh

# Test Phase 1 (WebFinger, Actor, Inbox)
./scripts/test-phase1-local.sh

# Test Phase 2 (Posts, Outbox, Delivery)
./scripts/test-phase2-local.sh

# Create and manage posts
dais post create "Hello, Fediverse!"
dais post list
dais post delete <post-id>

# Run unit tests
cd cli && pytest -v
cd workers/outbox && cargo test
```

**Not yet implemented:**
- [ ] Media attachment handling (R2 upload) - deferred to Phase 2.5

### Phase 3 - Interactions
- [ ] Receive and display replies
- [ ] Like/favorite support
- [ ] Boost/announce support
- [ ] Direct messages

### Phase 4 - Management
- [ ] Complete CLI tooling
- [ ] Analytics and reporting
- [ ] Federation testing suite

## ActivityPub Resources

- [ActivityPub Specification](https://www.w3.org/TR/activitypub/)
- [WebFinger RFC 7033](https://tools.ietf.org/html/rfc7033)
- [HTTP Signatures](https://tools.ietf.org/html/draft-cavage-http-signatures)
- [Mastodon ActivityPub Guide](https://docs.joinmastodon.org/spec/activitypub/)
- [ActivityPub Implementer's Guide](https://socialhub.activitypub.rocks/)

## Domain Configuration

Set up in Cloudflare DNS:

- `dais.social` → Cloudflare Pages (landing page)
- `social.dais.social` → Cloudflare Workers (ActivityPub endpoints)
- Add CNAME/A records as needed for WebFinger and Actor endpoints

## License

MIT

## Contributing

This is a personal project, but feel free to fork and adapt for your own single-user ActivityPub server!
