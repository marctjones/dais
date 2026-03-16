#!/bin/bash
# Test script to verify all refactored workers compile successfully

set -e

echo "========================================="
echo "Testing all refactored Cloudflare Workers"
echo "========================================="
echo ""

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

FAILED=0
PASSED=0

test_worker() {
    local worker=$1
    local worker_name=$(basename "$worker")

    echo -n "Testing $worker_name... "

    cd "$worker"
    if cargo check --quiet 2>/dev/null; then
        echo -e "${GREEN}✓ PASS${NC}"
        ((PASSED++))
    else
        echo -e "${RED}✗ FAIL${NC}"
        ((FAILED++))
        cargo check 2>&1 | tail -10
    fi
    cd - > /dev/null
}

# Test core library first
echo "Testing dais-core library..."
cd core
if cargo check --quiet 2>/dev/null; then
    echo -e "${GREEN}✓ dais-core PASS${NC}"
else
    echo -e "${RED}✗ dais-core FAIL${NC}"
    exit 1
fi
cd ..
echo ""

# Test Cloudflare bindings
echo "Testing dais-cloudflare bindings..."
cd platforms/cloudflare/bindings
if cargo check --quiet 2>/dev/null; then
    echo -e "${GREEN}✓ dais-cloudflare PASS${NC}"
else
    echo -e "${RED}✗ dais-cloudflare FAIL${NC}"
    exit 1
fi
cd ../../..
echo ""

# Test all workers
echo "Testing all workers:"
echo ""

for worker in platforms/cloudflare/workers/*/; do
    if [ -f "$worker/Cargo.toml" ]; then
        test_worker "$worker"
    fi
done

echo ""
echo "========================================="
echo "Test Summary"
echo "========================================="
echo -e "Passed: ${GREEN}$PASSED${NC}"
echo -e "Failed: ${RED}$FAILED${NC}"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All workers compiled successfully!${NC}"
    exit 0
else
    echo -e "${RED}Some workers failed to compile${NC}"
    exit 1
fi
