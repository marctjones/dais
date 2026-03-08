#!/usr/bin/env bash
set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

SESSION_NAME="dais-dev"

echo -e "${YELLOW}Stopping dais local development environment...${NC}"

# Check if tmux session exists
if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
    echo -e "${RED}Tmux session '$SESSION_NAME' does not exist.${NC}"
    exit 1
fi

# Kill the tmux session (this will terminate all workers)
tmux kill-session -t "$SESSION_NAME"

echo -e "${GREEN}✓ Tmux session '$SESSION_NAME' stopped successfully${NC}"
echo "All workers have been terminated."
