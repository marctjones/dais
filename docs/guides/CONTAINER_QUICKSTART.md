# Container Quick Start Guide

Get started with containerized testing in 30 seconds.

## Prerequisites

Install either:
- **Docker**: `sudo apt install docker.io docker-compose` (or from [docker.com](https://docs.docker.com/get-docker/))
- **Podman**: `sudo apt install podman podman-compose` (rootless, more secure)

## 1. Quick Test

```bash
# Start everything
make up

# Seed database
make seed

# Run all tests
make test
```

Done! You just ran 35 tests (Phase 1 + Phase 2 + unit tests) in a completely isolated environment.

## 2. Interactive Development

```bash
# Open a shell in the CLI container
make shell

# Inside the container, run dais commands:
dais stats
dais post create "Hello from containers!"
dais post list
dais followers list
```

## 3. Debugging

```bash
# Watch logs from all services
make logs

# Watch logs from specific worker
make logs-worker WORKER=webfinger
make logs-worker WORKER=outbox

# Check service status
make status
```

## 4. Clean Restart

```bash
# Complete fresh start (removes all data)
make clean
make up
make seed
```

## Common Workflows

### Before Committing Code
```bash
# Run full test suite with clean state
make test-clean
```

### Testing Database Changes
```bash
# Wipe database and reseed
make clean
make up
make seed
```

### Quick Iteration
```bash
# For development, use tmux instead (faster)
./scripts/dev-start.sh
# ... make changes, hot reload works ...
./scripts/dev-stop.sh

# Then verify with containers before commit
make test-clean
```

## All Make Commands

| Command | What it does |
|---------|-------------|
| `make up` | Start all services |
| `make down` | Stop all services |
| `make build` | Build container images |
| `make seed` | Seed database with test data |
| `make test` | Run all tests |
| `make test-clean` | Clean + rebuild + test |
| `make shell` | Open CLI shell |
| `make logs` | Tail all logs |
| `make logs-worker WORKER=<name>` | Tail specific worker |
| `make clean` | Remove all containers + volumes |
| `make restart` | Stop and start |
| `make rebuild` | Clean rebuild |
| `make status` | Show service status |

## Without Make

If you don't have `make` installed:

```bash
# Start
docker-compose up -d

# Seed
./scripts/seed-container-db.sh

# Test
./scripts/test-containers.sh

# Shell
docker-compose exec cli /bin/bash

# Stop
docker-compose down

# Clean
docker-compose down -v
```

## Ports

| Service | Port | URL |
|---------|------|-----|
| WebFinger | 8787 | http://localhost:8787/.well-known/webfinger |
| Actor | 8788 | http://localhost:8788/users/marc |
| Inbox | 8789 | http://localhost:8789/users/marc/inbox |
| Outbox | 8790 | http://localhost:8790/users/marc/outbox |

## Troubleshooting

**Services won't start:**
```bash
docker-compose logs
```

**Port already in use:**
```bash
docker-compose down
sudo lsof -i :8787-8790
```

**Database not seeding:**
```bash
docker-compose restart db-init
make seed
```

**Stale state:**
```bash
make clean && make up
```

## Why Containers?

- ✅ **Clean slate every time** - No contamination from previous runs
- ✅ **Reproducible** - Works identically on every machine
- ✅ **Isolated** - No dependency conflicts with host
- ✅ **CI/CD ready** - Same containers in CI as local
- ✅ **Easy reset** - `make clean && make up` = pristine state

## Hybrid Workflow (Recommended)

```bash
# Daily development (fast, hot reload)
./scripts/dev-start.sh
# ... code changes ...
./scripts/dev-stop.sh

# Before committing (verify clean state)
make test-clean
```

See [DEVELOPMENT.md](DEVELOPMENT.md) for full documentation.
