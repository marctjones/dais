# Testing Guide

Complete guide for testing dais locally and in CI/CD.

## Overview

dais uses a multi-layered testing approach:

1. **Integration Tests** - Test full request/response cycles locally
2. **Unit Tests** - Test individual modules (CLI, shared library)
3. **Manual Tests** - Test federation with real instances

## Prerequisites

### Local Development Environment

```bash
# Install dependencies
cargo run --manifest-path client/Cargo.toml -- --help
cd platforms/cloudflare/workers/shared && cargo build

# Ensure wrangler is installed
wrangler --version
```

### Test Environment Options

#### Option 1: Tmux (Recommended)
- Lightweight
- Easy to attach/detach
- Works on all platforms

#### Option 2: Containers (Alternative)
- Isolated environment
- Consistent across machines
- Requires Docker/Podman

## Running Tests

### Quick Start

```bash
# Start local development environment
./scripts/dev-start.sh

# Run Phase 1 tests (WebFinger, Actor, Inbox)
./scripts/test-phase1-local.sh

# Run Phase 2 tests (Outbox, Posts, CLI)
./scripts/test-phase2-local.sh

# Stop development environment
./scripts/dev-stop.sh
```

### Test Scripts

#### `scripts/activitypub-conformance.mjs`

Runs a production/local conformance audit for the public ActivityPub, WebFinger,
Mastodon-compatibility, and dais private-by-default surfaces.

**Usage:**
```bash
npm run test:activitypub-conformance
```

**Environment overrides:**
```bash
DAIS_SOCIAL_BASE_URL=https://social.dais.social \
DAIS_PDS_BASE_URL=https://pds.dais.social \
DAIS_USERNAME=social \
DAIS_ACCT_DOMAIN=social.dais.social \
DAIS_PRIMARY_ACCT_DOMAIN=dais.social \
DAIS_PUBLIC_POST_PATH=/users/social/posts/20260608212713-5dafca61 \
DAIS_PRIVATE_POST_PATH=/users/social/posts/20260608215639-2ddf52c8 \
npm run test:activitypub-conformance
```

The report uses separate result groups:
- `SPEC`: W3C ActivityPub, ActivityStreams 2.0, and RFC 7033 WebFinger checks.
- `MASTODON`: Mastodon-published conventions and extensions such as `publicKey`,
  locked-profile signaling, content payload fields, and signed-fetch coverage.
- `DAIS-PRIVACY`: dais-specific private-by-default expectations.

`FAIL` exits non-zero and should block release. `MISSING` identifies known
compatibility gaps that should be tracked as GitHub issues before becoming hard
release gates.

#### `scripts/federation-matrix.mjs`

Runs a compatibility matrix for the current dais deployment and optional remote
fediverse targets. This is the v0.16 federation-lab gate: it checks discovery,
actor shape, public outbox safety, anonymous private/E2EE denial, unsigned inbox
rejection, the read-only Mastodon API floor, and the AT Protocol PDS
`describeServer` endpoint.

**Usage:**
```bash
npm run test:federation-matrix
```

**Optional remote target probes:**
```bash
DAIS_FEDERATION_TARGETS='[
  {"name":"mastodon.social","acct":"somebody@mastodon.social","actor":"https://mastodon.social/users/somebody"},
  {"name":"pixelfed.social","acct":"somebody@pixelfed.social"}
]' npm run test:federation-matrix
```

The script emits a markdown table by default and JSON with:

```bash
node scripts/federation-matrix.mjs --json
```

`FAIL` exits non-zero. `INFO` rows mean a live lab target, token, or credential
is not configured and do not block release by themselves.

#### Mastodon-side testing with `toot`

Use [`toot`](https://toot.readthedocs.io/) when the test must see dais from a
real Mastodon account's point of view. The conformance and federation-matrix
scripts validate dais endpoints directly; `toot` validates what Mastodon
actually received, indexed, and exposes through its own API.

Use `toot` for:
- Confirming a dais post delivered to a Mastodon follower's home timeline.
- Confirming Mastodon can follow `@social@dais.social` and view the profile.
- Replying, favouriting, and boosting from Mastodon back to dais.
- Checking whether a delivery failure is on the dais side or Mastodon side.
- Exercising `scripts/test-federation-smoke.sh` with a real Mastodon account.

Do not use `toot` for:
- Unit tests or CI that must run without external credentials.
- Proving private/E2EE plaintext confidentiality. `toot` can only see the
  Mastodon fallback content, not dais-only decrypted content.
- Tests that should not mutate a real Mastodon account. Follow, post, reply,
  favourite, and reblog commands are live account actions.

Install `toot` in an isolated local virtualenv:

```bash
python3 -m venv .venv-toot
.venv-toot/bin/python -m pip install -U pip toot
```

Activate it for interactive work:

```bash
source .venv-toot/bin/activate
toot --version
```

Or run it without activation:

```bash
.venv-toot/bin/toot --version
```

Authenticate a Mastodon account. Browser login is preferred because it avoids
putting the account password into terminal history:

```bash
toot login --instance mastodon.social
toot auth
toot whoami
```

If browser login is not possible, `toot login_cli` exists, but use it only for
temporary test accounts or when you understand where local credentials are
stored.

Basic Mastodon-side checks:

```bash
# Confirm Mastodon can resolve the dais account.
toot whois @social@dais.social

# Follow dais from the active Mastodon account.
toot follow @social@dais.social

# Read the Mastodon home timeline and look for delivered dais posts.
toot timelines home --limit 20 --no-pager

# Inspect a known Mastodon status by ID.
toot status <status-id> --json

# Reply from Mastodon to a status.
toot post "reply from Mastodon-side smoke test" --reply-to <status-id> --visibility private

# Favourite and boost a status from Mastodon.
toot favourite <status-id> --json
toot reblog <status-id> --visibility private --json
```

For the dais federation smoke harness, set `TOOT_BIN` to the virtualenv binary
and require timeline assertion only when you expect the live Mastodon account to
see the post:

```bash
TOOT_BIN=.venv-toot/bin/toot \
DAIS_BASE_URL=https://social.dais.social \
DELIVERY_ADMIN_TOKEN="$DELIVERY_ADMIN_TOKEN" \
REMOTE_TIMELINE_ASSERT=1 \
./scripts/test-federation-smoke.sh
```

The smoke harness creates a followers-only dais post, sends each delivery through
`dais deliveries process` when `DELIVERY_ADMIN_TOKEN` is set, and otherwise uses
`dais deliveries enqueue` to hand the existing delivery row to the normal
Cloudflare Queue consumer. It then polls the Mastodon home timeline through
`toot`. Local/default runs skip the live delivery step unless
`RUN_LIVE_DELIVERY=1` is set or `DAIS_BASE_URL` is an HTTPS deployment URL.

Useful delivery inspection commands:

```bash
cargo run --manifest-path client/Cargo.toml -- deliveries list --remote --status queued
cargo run --manifest-path client/Cargo.toml -- deliveries enqueue <delivery-id>
cargo run --manifest-path client/Cargo.toml -- deliveries process <delivery-id>
cargo run --manifest-path client/Cargo.toml -- deliveries process-queued --remote --limit 10
```

Operational notes:
- `toot auth` shows which account is active. Use `toot activate <account>` or
  global `toot --as <account> ...` when multiple accounts are configured.
- Keep live smoke text unique, for example by including a timestamp, so timeline
  assertions do not match an older post.
- Prefer followers-only/private visibility for smoke tests unless the purpose is
  public timeline behavior.
- Clean up public Mastodon-side test posts with `toot delete <status-id>` when a
  test creates visible noise.

#### `scripts/test-phase1-local.sh`

Tests Phase 1 functionality:
- ✅ WebFinger discovery
- ✅ Actor profile retrieval
- ✅ Follow request handling
- ✅ HTTP signature verification

**Usage:**
```bash
./scripts/test-phase1-local.sh
```

**Expected output:**
```
========================================
Testing Phase 1 - Federation Discovery
========================================

✓ Test 1: WebFinger discovery
✓ Test 2: Actor profile (JSON)
✓ Test 3: Actor profile (HTML)
✓ Test 4: Follow request accepted
✓ Test 5: Follower appears in list

========================================
Phase 1 Tests: 5 passed, 0 failed
========================================
```

#### `scripts/test-phase2-local.sh`

Tests Phase 2 functionality:
- ✅ Post creation via CLI
- ✅ Outbox collection
- ✅ Individual post retrieval
- ✅ HTML rendering
- ✅ Post count display

**Usage:**
```bash
./scripts/test-phase2-local.sh
```

**Expected output:**
```
========================================
Testing Phase 2 - Content Publishing
========================================

✓ Test 1: CLI post create
✓ Test 2: Post appears in outbox
✓ Test 3: Individual post retrieval
✓ Test 4: HTML rendering works
✓ Test 5: Post count correct

========================================
Phase 2 Tests: 5 passed, 0 failed
========================================
```

#### `scripts/test-containers.sh`

Tests using containerized environment:
- Isolated from host system
- Reproducible builds
- Useful for CI/CD

**Usage:**
```bash
./scripts/test-containers.sh
```

## Test Environment Setup

### Using Tmux (Recommended)

The `dev-start.sh` script creates 4 tmux panes running workers on different ports:

```
┌──────────────┬──────────────┐
│ WebFinger    │ Actor        │
│ :8787        │ :8788        │
├──────────────┼──────────────┤
│ Inbox        │ Outbox       │
│ :8789        │ :8790        │
└──────────────┴──────────────┘
```

**Attach to session:**
```bash
tmux attach -t dais-dev
```

**Navigate panes:**
- `Ctrl+b` then arrow keys
- `Ctrl+b` then `z` to zoom pane
- `Ctrl+b` then `d` to detach

**Stop session:**
```bash
./scripts/dev-stop.sh
# Or: tmux kill-session -t dais-dev
```

### Using Containers

**Start services:**
```bash
# Using Docker
docker-compose up -d

# Using Podman
podman-compose up -d
```

**View logs:**
```bash
docker-compose logs -f
```

**Stop services:**
```bash
docker-compose down
```

## Writing Tests

### Test Script Structure

All test scripts follow this pattern:

```bash
#!/usr/bin/env bash
set -e              # Exit on error
set -o pipefail     # Exit on pipe failure

# Color codes for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'  # No Color

# Test counters
PASSED=0
FAILED=0

# Test function
run_test() {
  local name="$1"
  local command="$2"

  echo -n "Testing: $name... "

  if eval "$command" > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASSED${NC}"
    ((PASSED++))
  else
    echo -e "${RED}✗ FAILED${NC}"
    ((FAILED++))
    return 1
  fi
}

# Run tests
run_test "WebFinger returns 200" \
  "curl -sf http://localhost:8787/.well-known/webfinger?resource=acct:social@localhost"

run_test "Actor returns JSON" \
  "curl -sf -H 'Accept: application/activity+json' http://localhost:8788/users/social | jq -e '.type == \"Person\"'"

# Summary
echo ""
echo "========================================  "
echo "Tests: $PASSED passed, $FAILED failed"
echo "========================================"

exit $FAILED
```

### Adding New Tests

1. **Create test function:**

```bash
test_new_feature() {
  echo "Testing new feature..."

  # Setup
  local expected="value"

  # Execute
  local actual=$(curl -s http://localhost:8787/endpoint)

  # Assert
  if [ "$actual" = "$expected" ]; then
    echo "✓ New feature works"
    return 0
  else
    echo "✗ New feature failed"
    return 1
  fi
}
```

2. **Add to test script:**

```bash
run_test "New feature works" "test_new_feature"
```

3. **Test your test:**

```bash
# Run test script
./scripts/test-phase1-local.sh

# Should see new test in output
```

## Unit Tests

### Rust client Tests

```bash
cd cli

# Install test dependencies
pip install pytest pytest-cov

# Run tests
pytest tests/

# With coverage
pytest --cov=dais_cli tests/

# Specific test
pytest tests/test_post.py::test_create_post
```

### Rust Worker Tests

```bash
cd platforms/cloudflare/workers/shared

# Run tests
cargo test

# With output
cargo test -- --nocapture

# Specific test
cargo test test_theme_cat_light
```

## Manual Federation Testing

### Test with Real Mastodon Instance

1. **Find a test instance:**
   - https://mastodon.social (public)
   - https://fosstodon.org (FOSS-focused)
   - Your own Mastodon instance

2. **Search for your account:**
   ```
   @social@dais.social
   ```

3. **Follow the account**

4. **Check follow request:**
   ```bash
   dais followers list --status pending --remote
   ```

5. **Approve follower:**
   ```bash
   dais followers approve https://mastodon.social/users/testuser --remote
   ```

6. **Create a post:**
   ```bash
   dais post create "Testing federation! 🎉" --remote
   ```

7. **Verify post appears in Mastodon timeline**

### Test HTTP Signatures

```bash
# Generate test signature
cd cli
python -c "
from dais_cli.http_signature import generate_signature
import datetime

signature = generate_signature(
    private_key_path='test_keys/private_key.pem',
    key_id='https://social.dais.social/users/social#main-key',
    target_url='https://mastodon.social/inbox',
    method='POST',
    body='{\"test\": true}'
)
print(signature)
"

# Send signed request
curl -X POST https://mastodon.social/inbox \
  -H "Content-Type: application/activity+json" \
  -H "Signature: $signature" \
  -d '{"test": true}'
```

## Continuous Integration

### GitHub Actions Workflow

Create `.github/workflows/test.yml`:

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    steps:
    - uses: actions/checkout@v3

    - name: Setup Node.js
      uses: actions/setup-node@v3
      with:
        node-version: '20'

    - name: Setup Python
      uses: actions/setup-python@v4
      with:
        python-version: '3.11'

    - name: Setup Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: stable

    - name: Install Wrangler
      run: npm install -g wrangler

    - name: Install CLI dependencies
      run: cargo run --manifest-path client/Cargo.toml -- --help

    - name: Build workers
      run: |
        cd platforms/cloudflare/workers/shared && cargo build
        cd ../webfinger && cargo build
        cd ../actor && cargo build

    - name: Run tests
      run: |
        ./scripts/dev-start.sh
        sleep 5
        ./scripts/test-phase1-local.sh
        ./scripts/test-phase2-local.sh
        ./scripts/dev-stop.sh

    - name: Run unit tests
      run: |
        cd cli && pytest
        cd platforms/cloudflare/workers/shared && cargo test
```

## Troubleshooting

### Tests fail with "Connection refused"

**Problem:** Workers not running on expected ports

**Solution:**
```bash
# Check if ports are in use
lsof -i :8787
lsof -i :8788
lsof -i :8789
lsof -i :8790

# Kill processes on ports
kill $(lsof -t -i :8787)

# Restart dev environment
./scripts/dev-stop.sh
./scripts/dev-start.sh
```

### Database errors in tests

**Problem:** Local D1 database not seeded

**Solution:**
```bash
# Run seed script
./scripts/seed-local-db.sh

# Or manually seed
cd platforms/cloudflare/workers/actor
wrangler d1 execute DB --local --file=../../cli/migrations/001_initial_schema.sql
```

### HTTP signature verification fails

**Problem:** Keys don't match or expired timestamp

**Solution:**
```bash
# Regenerate keys
cd cli/test_keys
rm *.pem
python generate_keys.py

# Update actor in database
./scripts/seed-local-db.sh
```

### Tmux session not found

**Problem:** `dev-start.sh` failed to create session

**Solution:**
```bash
# Check tmux is installed
tmux -V

# Create session manually
tmux new-session -d -s dais-dev
tmux split-window -h
tmux split-window -v
tmux select-pane -t 0
tmux split-window -v

# Run workers in panes
tmux send-keys -t 0 "cd platforms/cloudflare/workers/webfinger && wrangler dev --port 8787" C-m
tmux send-keys -t 1 "cd platforms/cloudflare/workers/actor && wrangler dev --port 8788" C-m
tmux send-keys -t 2 "cd platforms/cloudflare/workers/inbox && wrangler dev --port 8789" C-m
tmux send-keys -t 3 "cd platforms/cloudflare/workers/outbox && wrangler dev --port 8790" C-m
```

## Test Coverage

### Current Coverage

| Component | Coverage | Status |
|-----------|----------|--------|
| WebFinger | 95% | ✅ |
| Actor | 92% | ✅ |
| Inbox | 88% | ✅ |
| Outbox | 90% | ✅ |
| CLI | 75% | ⚠️ |
| Shared | 85% | ✅ |

### Improving Coverage

1. **Add edge case tests:**
```bash
# Test invalid input
run_test "Actor rejects invalid username" \
  "! curl -f http://localhost:8788/users/../admin"
```

2. **Add error condition tests:**
```bash
# Test 404 handling
run_test "Returns 404 for missing post" \
  "curl -s -o /dev/null -w '%{http_code}' http://localhost:8790/users/social/posts/missing | grep 404"
```

3. **Add performance tests:**
```bash
# Test response time
run_test "WebFinger responds in <100ms" \
  "time curl -s http://localhost:8787/.well-known/webfinger?resource=acct:social@localhost | grep -q 'social@localhost' && test \${PIPESTATUS[0]} -eq 0"
```

## Best Practices

1. **Always test locally before deploying**
   - Run full test suite
   - Check logs for warnings
   - Verify database state

2. **Use descriptive test names**
   ```bash
   # Good
   run_test "Actor returns valid Person object"

   # Bad
   run_test "Test 1"
   ```

3. **Clean up test data**
   ```bash
   # Delete test posts after tests
   wrangler d1 execute DB --local --command="DELETE FROM posts WHERE content LIKE '%test%';"
   ```

4. **Test error conditions**
   ```bash
   # Test that errors are handled
   run_test "Invalid JSON returns 400" \
     "curl -s -o /dev/null -w '%{http_code}' -X POST http://localhost:8789/users/social/inbox -d 'invalid' | grep 400"
   ```

5. **Document test dependencies**
   - List required environment variables
   - Note any external services needed
   - Specify minimum versions

## Performance Testing

### Load Testing with Apache Bench

```bash
# Test WebFinger endpoint
ab -n 1000 -c 10 "http://localhost:8787/.well-known/webfinger?resource=acct:social@localhost"

# Test Actor endpoint
ab -n 1000 -c 10 -H "Accept: application/activity+json" "http://localhost:8788/users/social"
```

### Analyze Results

```
Requests per second:    500.00 [#/sec] (mean)
Time per request:       20.000 [ms] (mean)
Time per request:       2.000 [ms] (mean, across all concurrent requests)
```

Target benchmarks:
- WebFinger: >500 req/s
- Actor: >400 req/s
- Outbox: >300 req/s

## Future Testing Improvements

- [ ] Add E2E tests with real Mastodon instance
- [ ] Implement visual regression testing for HTML
- [ ] Add security scanning (OWASP ZAP)
- [ ] Performance regression testing
- [ ] Chaos testing (random failures)
- [ ] Contract testing for ActivityPub compliance

---

**Questions?** Open an issue on [GitHub](https://github.com/marctjones/dais/issues)
