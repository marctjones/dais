# Local Development Guide

This guide covers setting up and using the local development environment for dais.

## Two Approaches

### 🐳 Containerized (Recommended for Testing)
- **Clean, isolated, reproducible**
- Perfect for running tests
- No local dependencies except Docker/Podman
- Easy reset: `make clean && make up`

### ⚡ Tmux-based (Recommended for Development)
- **Fast iteration, hot reload**
- Better for active development
- Requires Rust, wrangler, Python installed
- Ideal for debugging

---

## Containerized Development (Docker/Podman)

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) or [Podman](https://podman.io/getting-started/installation)
- [docker-compose](https://docs.docker.com/compose/install/) or [podman-compose](https://github.com/containers/podman-compose)
- [make](https://www.gnu.org/software/make/) (optional, for convenience)

### Quick Start with Containers

```bash
# Build and start all services
make up

# Seed database with test data
make seed

# Run all tests
make test

# Open a shell in the CLI container
make shell

# Stop everything
make down
```

### Detailed Container Commands

```bash
# Build container images
make build
# or: docker-compose build

# Start services (detached)
make up
# or: docker-compose up -d

# Seed database
make seed
# or: ./scripts/seed-container-db.sh

# Run tests (Phase 1 + Phase 2 + unit tests)
make test
# or: ./scripts/test-containers.sh

# Clean start (removes volumes)
make clean
make up
# or: docker-compose down -v && docker-compose up -d

# View logs
make logs
# or: docker-compose logs -f

# View logs for specific worker
make logs-worker WORKER=webfinger
# or: docker-compose logs -f webfinger

# Open shell in CLI container
make shell
# or: docker-compose exec cli /bin/bash

# Run dais commands in container
docker-compose exec cli dais stats
docker-compose exec cli dais post create "Hello from container!"
docker-compose exec cli dais post list

# Stop all services
make down
# or: docker-compose down

# Clean rebuild
make rebuild
# or: docker-compose down -v && docker-compose build && docker-compose up -d
```

### Container Testing Workflow

```bash
# Fresh test run (recommended)
make test-clean

# This will:
# 1. Stop and remove all containers + volumes
# 2. Build fresh images
# 3. Start all services
# 4. Seed database
# 5. Run Phase 1 tests (10 tests)
# 6. Run Phase 2 tests (10 tests)
# 7. Run Python unit tests (15 tests)
```

### Container Services

| Service | Port | Purpose |
|---------|------|---------|
| webfinger | 8787 | WebFinger discovery endpoint |
| actor | 8788 | ActivityPub Actor profile |
| inbox | 8789 | Receive ActivityPub activities |
| outbox | 8790 | Serve posts (OrderedCollection) |
| cli | - | Run dais commands and tests |
| db-init | - | Initialize and migrate D1 database |

### Shared D1 Database

All workers share the same D1 database via a Docker volume:
- Volume: `d1-data`
- Mounted to: `/app/workers/actor/.wrangler` in each container
- Ensures consistent state across all workers

---

## Tmux-based Development (Native)

### Prerequisites

- [Rust](https://rustup.rs/) 1.70+
- [wrangler](https://developers.cloudflare.com/workers/wrangler/) 3.0+
- [Python](https://www.python.org/) 3.10+
- [tmux](https://github.com/tmux/tmux/wiki) (for running multiple workers)
- [curl](https://curl.se/) (for testing)

## Quick Start

### 1. Install Python CLI

```bash
cd cli
pip install -e .
```

### 2. Start Local Environment

```bash
# Start all workers in tmux
./scripts/dev-start.sh

# In another terminal, seed the database
./scripts/seed-local-db.sh
```

This will:
- Launch 4 Cloudflare Workers in local mode (tmux windows)
- Create local D1 database in `.wrangler/state/v3/d1/`
- Seed with test actor `marc@localhost` and sample followers

### 3. Test the Setup

```bash
# Test WebFinger discovery
curl "http://localhost:8787/.well-known/webfinger?resource=acct:marc@localhost"

# Test Actor endpoint
curl -H "Accept: application/activity+json" "http://localhost:8788/users/marc"

# Test Stats command
dais stats

# Or run the full Phase 1 test suite
./scripts/test-phase1-local.sh
```

### 4. Attach to Workers

```bash
# Attach to the tmux session to see worker logs
tmux attach -t dais-dev

# Switch between windows:
# Ctrl+b, 0 = WebFinger worker
# Ctrl+b, 1 = Actor worker
# Ctrl+b, 2 = Inbox worker
# Ctrl+b, 3 = Outbox worker
# Ctrl+b, 4 = Shell (for running commands)

# Detach from tmux (leave it running)
# Press: Ctrl+b, d
```

### 5. Stop Environment

```bash
./scripts/dev-stop.sh
```

## Worker Port Assignments

| Worker | Port | Endpoint |
|--------|------|----------|
| WebFinger | 8787 | `http://localhost:8787/.well-known/webfinger` |
| Actor | 8788 | `http://localhost:8788/users/:username` |
| Inbox | 8789 | `http://localhost:8789/users/:username/inbox` |
| Outbox | 8790 | `http://localhost:8790/users/:username/outbox` |

## Local Database

### Location

Local D1 databases are stored in:
```
.wrangler/state/v3/d1/miniflare-D1DatabaseObject/
```

Each worker has its own `.wrangler/` directory with a local SQLite database.

### Running Migrations

```bash
cd workers/actor
wrangler d1 execute DB --local --file="../../cli/migrations/001_initial_schema.sql"
```

Or use the seed script which runs migrations automatically:
```bash
./scripts/seed-local-db.sh
```

### Querying Local Database

```bash
cd workers/actor
wrangler d1 execute DB --local --command="SELECT * FROM actors;"
wrangler d1 execute DB --local --command="SELECT * FROM followers;"
wrangler d1 execute DB --local --command="SELECT * FROM posts;"
```

### Resetting Database

```bash
# Delete the local database
rm -rf workers/actor/.wrangler/state/
rm -rf workers/inbox/.wrangler/state/
rm -rf workers/outbox/.wrangler/state/
rm -rf workers/webfinger/.wrangler/state/

# Re-seed
./scripts/seed-local-db.sh
```

## Test Data

The `seed-local-db.sh` script creates:

### Test Actor
- **Username**: `marc`
- **Full ID**: `acct:marc@localhost` / `https://localhost/users/marc`
- **Keys**: Uses keypair from `cli/test_keys/`

### Sample Followers

| User | Instance | Status | Purpose |
|------|----------|--------|---------|
| alice | mastodon.social | approved | Test approved follower |
| bob | pleroma.example.com | approved | Test second approved follower |
| charlie | pixelfed.social | pending | Test pending follow request |
| dave | mastodon.example.com | rejected | Test rejected follower |

### Sample Post

One sample post is created with:
- **ID**: `https://localhost/users/marc/posts/001`
- **Content**: "Hello from local dais development! This is a test post."
- **Visibility**: public

## Testing Phase 1 (Basic Federation)

### WebFinger Discovery

```bash
# Discover actor
curl "http://localhost:8787/.well-known/webfinger?resource=acct:marc@localhost"

# Expected: JSON with actor URL and links
```

### Actor Profile

```bash
# Get actor profile
curl -H "Accept: application/activity+json" "http://localhost:8788/users/marc"

# Expected: ActivityPub Person object with public key
```

### Follower Management

```bash
# List followers
dais followers list

# Expected: Shows alice (approved), bob (approved), charlie (pending), dave (rejected)

# Approve pending follower
dais followers approve charlie@pixelfed.social

# Reject a follower
dais followers reject charlie@pixelfed.social
```

### Statistics

```bash
# Show follower counts and activity stats
dais stats

# Expected: Shows approved: 2, pending: 1, rejected: 1
```

## Testing Phase 2 (Content Publishing)

### Create Posts

```bash
# Create a public post
dais post create "Hello, Fediverse!"

# Create an unlisted post
dais post create "Unlisted post" --visibility unlisted

# Create a followers-only post
dais post create "Followers only" --visibility followers
```

### List Posts

```bash
# List all posts
dais post list

# Expected: Table with post IDs, content preview, visibility, publish date
```

### Delete Posts

```bash
# Delete a post
dais post delete <post-id>

# Expected: Post marked as deleted, Delete activity sent to followers
```

### Outbox Endpoint

```bash
# Get outbox collection
curl -H "Accept: application/activity+json" "http://localhost:8790/users/marc/outbox"

# Expected: OrderedCollection with posts

# Get individual post
curl -H "Accept: application/activity+json" "http://localhost:8790/users/marc/posts/001"

# Expected: Note object
```

## Running Tests

### Unit Tests

```bash
# Python CLI tests
cd cli
pytest -v

# Rust worker tests
cd workers/shared
cargo test

cd workers/inbox
cargo test

cd workers/outbox
cargo test
```

### Integration Tests

```bash
# Test Phase 1 (WebFinger, Actor, Inbox, Followers)
./scripts/test-phase1-local.sh

# Test Phase 2 (Posts, Outbox, Delivery)
./scripts/test-phase2-local.sh
```

## CLI Usage in Local Mode

The dais CLI automatically uses local mode when you don't pass the `--remote` flag.

### Configuration

The CLI looks for configuration in `~/.dais/config.toml`. For local development, you can override values:

```toml
[server]
domain = "localhost"
activitypub_domain = "localhost"
username = "marc"

[cloudflare]
account_id = "test-account-id"
api_token = "test-token"
d1_database_id = "test-db-id"

[keys]
private_key_path = "/home/marc/Projects/dais/cli/test_keys/private.pem"
public_key_path = "/home/marc/Projects/dais/cli/test_keys/public.pem"
```

### Local Commands

```bash
# All commands default to local mode
dais stats                           # Query local D1
dais followers list                  # Query local D1
dais post create "Test post"         # Insert to local D1

# Add --remote flag to use production
dais stats --remote                  # Query production D1
dais followers list --remote         # Query production D1
```

## Worker Development

### Rebuilding Workers

Workers are automatically rebuilt by `wrangler dev`, but you can manually build:

```bash
cd workers/webfinger
cargo build --target wasm32-unknown-unknown

# Or let wrangler handle it
wrangler dev --local
```

### Adding Dependencies

```bash
cd workers/shared
cargo add serde_json

# Update all workers that depend on shared
cd ../webfinger && cargo update
cd ../actor && cargo update
cd ../inbox && cargo update
cd ../outbox && cargo update
```

### Debugging

Workers log to stdout in the tmux windows. To see logs:

```bash
# Attach to tmux session
tmux attach -t dais-dev

# Select a worker window (Ctrl+b, 0-3)
# Watch the logs in real-time
```

## Troubleshooting

### Workers won't start

- **Check if ports are in use**: `lsof -i :8787-8790`
- **Kill existing wrangler processes**: `pkill -f wrangler`
- **Delete .wrangler/state**: `rm -rf workers/*/.wrangler/state`

### Database errors

- **Re-run migrations**: `./scripts/seed-local-db.sh`
- **Check D1 database exists**: `ls workers/actor/.wrangler/state/v3/d1/`
- **Query database directly**: `cd workers/actor && wrangler d1 execute DB --local --command="SELECT * FROM actors;"`

### Signature verification failures

- **Check test keys exist**: `ls cli/test_keys/`
- **Verify keys in database**: `cd workers/actor && wrangler d1 execute DB --local --command="SELECT username, substr(public_key, 1, 50) FROM actors;"`
- **Re-seed database**: `./scripts/seed-local-db.sh`

### Tmux session issues

- **Kill stuck session**: `tmux kill-session -t dais-dev`
- **List all sessions**: `tmux ls`
- **Attach to session**: `tmux attach -t dais-dev`

### Container issues

- **Services not starting**: `docker-compose logs` to see errors
- **Port conflicts**: `docker-compose down` then check `lsof -i :8787-8790`
- **Database not seeding**: `docker-compose restart db-init && make seed`
- **Stale state**: `make clean && make up` for fresh start

---

## Choosing Your Workflow

### Use Containers When:
- ✅ Running tests (completely clean environment)
- ✅ Verifying functionality (reproducible results)
- ✅ CI/CD pipelines (identical to production)
- ✅ Onboarding new contributors (no dependency setup)
- ✅ Testing database migrations (easy reset)
- ✅ Sharing environment state (volume snapshots)

### Use Tmux When:
- ✅ Active development (hot reload on file changes)
- ✅ Debugging workers (direct console access)
- ✅ Quick iteration (no container rebuild)
- ✅ Testing performance (less overhead)
- ✅ Developing new features (faster feedback loop)

### Hybrid Workflow (Recommended):
```bash
# Daily development
./scripts/dev-start.sh
# ... make changes, hot reload works ...
./scripts/dev-stop.sh

# Before committing
make test-clean  # Fresh containerized test
```

---

## Next Steps

Once you've verified Phase 1 and Phase 2 work locally:

1. Test federation with real Mastodon instance using cloudflared tunnel (see `scripts/cloudflared-tunnel.sh`)
2. Deploy to Cloudflare Workers with `wrangler deploy`
3. Configure DNS for your domain
4. Test with real Fediverse servers

## Resources

- [Cloudflare Workers Docs](https://developers.cloudflare.com/workers/)
- [Wrangler CLI Reference](https://developers.cloudflare.com/workers/wrangler/commands/)
- [D1 Database Docs](https://developers.cloudflare.com/d1/)
- [ActivityPub Spec](https://www.w3.org/TR/activitypub/)
- [WebFinger RFC](https://tools.ietf.org/html/rfc7033)
