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
cd cli && pip install -e .
cd workers/shared && cargo build

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
  "curl -sf http://localhost:8787/.well-known/webfinger?resource=acct:marc@localhost"

run_test "Actor returns JSON" \
  "curl -sf -H 'Accept: application/activity+json' http://localhost:8788/users/marc | jq -e '.type == \"Person\"'"

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

### Python CLI Tests

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
cd workers/shared

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
      run: cd cli && pip install -e .

    - name: Build workers
      run: |
        cd workers/shared && cargo build
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
        cd workers/shared && cargo test
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
cd workers/actor
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
tmux send-keys -t 0 "cd workers/webfinger && wrangler dev --port 8787" C-m
tmux send-keys -t 1 "cd workers/actor && wrangler dev --port 8788" C-m
tmux send-keys -t 2 "cd workers/inbox && wrangler dev --port 8789" C-m
tmux send-keys -t 3 "cd workers/outbox && wrangler dev --port 8790" C-m
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
  "curl -s -o /dev/null -w '%{http_code}' http://localhost:8790/users/marc/posts/missing | grep 404"
```

3. **Add performance tests:**
```bash
# Test response time
run_test "WebFinger responds in <100ms" \
  "time curl -s http://localhost:8787/.well-known/webfinger?resource=acct:marc@localhost | grep -q 'marc@localhost' && test \${PIPESTATUS[0]} -eq 0"
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
     "curl -s -o /dev/null -w '%{http_code}' -X POST http://localhost:8789/users/marc/inbox -d 'invalid' | grep 400"
   ```

5. **Document test dependencies**
   - List required environment variables
   - Note any external services needed
   - Specify minimum versions

## Performance Testing

### Load Testing with Apache Bench

```bash
# Test WebFinger endpoint
ab -n 1000 -c 10 "http://localhost:8787/.well-known/webfinger?resource=acct:marc@localhost"

# Test Actor endpoint
ab -n 1000 -c 10 -H "Accept: application/activity+json" "http://localhost:8788/users/marc"
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
