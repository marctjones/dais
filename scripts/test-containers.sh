#!/usr/bin/env bash
set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${BLUE}════════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Containerized Testing for dais${NC}"
echo -e "${BLUE}════════════════════════════════════════════════${NC}"
echo ""

# Check if docker-compose or podman-compose is available
if command -v podman-compose &> /dev/null; then
    COMPOSE_CMD="podman-compose"
elif command -v docker-compose &> /dev/null; then
    COMPOSE_CMD="docker-compose"
else
    echo -e "${RED}Error: Neither podman-compose nor docker-compose found${NC}"
    exit 1
fi

echo -e "${BLUE}Using: $COMPOSE_CMD${NC}"
echo ""

# Clean start option
if [ "$1" == "--clean" ] || [ "$1" == "-c" ]; then
    echo -e "${YELLOW}Cleaning previous containers and volumes...${NC}"
    $COMPOSE_CMD down -v
    echo ""
fi

# Build images
echo -e "${BLUE}Building container images...${NC}"
$COMPOSE_CMD build

# Start services
echo -e "${BLUE}Starting services...${NC}"
$COMPOSE_CMD up -d

# Wait for health checks
echo -e "${BLUE}Waiting for services to be healthy...${NC}"
sleep 5

# Check service status
echo ""
$COMPOSE_CMD ps
echo ""

# Seed database
echo -e "${BLUE}Seeding database...${NC}"
./scripts/seed-container-db.sh

# Run Phase 1 tests
echo ""
echo -e "${BLUE}════════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Running Phase 1 Tests${NC}"
echo -e "${BLUE}════════════════════════════════════════════════${NC}"
echo ""

$COMPOSE_CMD exec -T cli bash -c "cd /app && ./scripts/test-phase1-local.sh"
PHASE1_EXIT=$?

# Run Phase 2 tests
echo ""
echo -e "${BLUE}════════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Running Phase 2 Tests${NC}"
echo -e "${BLUE}════════════════════════════════════════════════${NC}"
echo ""

$COMPOSE_CMD exec -T cli bash -c "cd /app && ./scripts/test-phase2-local.sh"
PHASE2_EXIT=$?

# Run Python unit tests
echo ""
echo -e "${BLUE}════════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Running Python Unit Tests${NC}"
echo -e "${BLUE}════════════════════════════════════════════════${NC}"
echo ""

$COMPOSE_CMD exec -T cli bash -c "cd /app/cli && pytest -v"
PYTEST_EXIT=$?

# Summary
echo ""
echo -e "${BLUE}════════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Test Summary${NC}"
echo -e "${BLUE}════════════════════════════════════════════════${NC}"
echo ""

if [ $PHASE1_EXIT -eq 0 ]; then
    echo -e "${GREEN}✓ Phase 1 tests: PASSED${NC}"
else
    echo -e "${RED}✗ Phase 1 tests: FAILED${NC}"
fi

if [ $PHASE2_EXIT -eq 0 ]; then
    echo -e "${GREEN}✓ Phase 2 tests: PASSED${NC}"
else
    echo -e "${RED}✗ Phase 2 tests: FAILED${NC}"
fi

if [ $PYTEST_EXIT -eq 0 ]; then
    echo -e "${GREEN}✓ Python unit tests: PASSED${NC}"
else
    echo -e "${RED}✗ Python unit tests: FAILED${NC}"
fi

echo ""

# Cleanup option
if [ "$2" == "--cleanup" ]; then
    echo -e "${YELLOW}Cleaning up containers...${NC}"
    $COMPOSE_CMD down
fi

# Exit with failure if any test failed
if [ $PHASE1_EXIT -ne 0 ] || [ $PHASE2_EXIT -ne 0 ] || [ $PYTEST_EXIT -ne 0 ]; then
    echo -e "${RED}Some tests failed${NC}"
    exit 1
else
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
fi
