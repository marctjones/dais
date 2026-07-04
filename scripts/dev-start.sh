#!/usr/bin/env bash
set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

SESSION_NAME="dais-dev"

echo -e "${BLUE}Starting dais local development environment...${NC}"

# Check if tmux session already exists
if tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
    echo -e "${YELLOW}Tmux session '$SESSION_NAME' already exists.${NC}"
    echo "Attach with: tmux attach -t $SESSION_NAME"
    exit 1
fi

# Get the project root directory (parent of scripts/)
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"
WORKERS_ROOT="$PROJECT_ROOT/platforms/cloudflare/workers"

echo -e "${GREEN}Project root: $PROJECT_ROOT${NC}"

# Create new tmux session in detached mode
tmux new-session -d -s "$SESSION_NAME" -c "$PROJECT_ROOT"

# Window 0: Landing worker (port 8787)
tmux rename-window -t "$SESSION_NAME:0" "landing"
tmux send-keys -t "$SESSION_NAME:0" "cd $WORKERS_ROOT/landing" C-m
tmux send-keys -t "$SESSION_NAME:0" "echo -e '${GREEN}Starting active Landing worker on port 8787...${NC}'" C-m
tmux send-keys -t "$SESSION_NAME:0" "wrangler dev --local --port 8787 --var DOMAIN=localhost --var ACTIVITYPUB_DOMAIN=localhost" C-m

# Window 1: Router worker (port 8788)
tmux new-window -t "$SESSION_NAME:1" -n "router" -c "$WORKERS_ROOT/router"
tmux send-keys -t "$SESSION_NAME:1" "echo -e '${GREEN}Starting active Router worker on port 8788...${NC}'" C-m
tmux send-keys -t "$SESSION_NAME:1" "wrangler dev --local --port 8788 --var DOMAIN=localhost --var ACTIVITYPUB_DOMAIN=localhost" C-m

# Window 2: Shell for running commands
tmux new-window -t "$SESSION_NAME:2" -n "shell" -c "$PROJECT_ROOT"
tmux send-keys -t "$SESSION_NAME:2" "echo -e '${BLUE}Welcome to dais development shell${NC}'" C-m
tmux send-keys -t "$SESSION_NAME:2" "echo -e 'Active workers running on:'" C-m
tmux send-keys -t "$SESSION_NAME:2" "echo -e '  Landing: http://localhost:8787'" C-m
tmux send-keys -t "$SESSION_NAME:2" "echo -e '  Router:  http://localhost:8788'" C-m
tmux send-keys -t "$SESSION_NAME:2" "echo ''" C-m
tmux send-keys -t "$SESSION_NAME:2" "echo -e 'Run ${GREEN}./scripts/deploy.sh list${NC} to see active and legacy worker status'" C-m
tmux send-keys -t "$SESSION_NAME:2" "echo -e 'Run ${GREEN}./scripts/deploy.sh build${NC} to validate active workers'" C-m

# Select the shell window
tmux select-window -t "$SESSION_NAME:2"

echo -e "${GREEN}✓ Tmux session '$SESSION_NAME' created successfully${NC}"
echo ""
echo "Workers starting on:"
echo "  Landing: http://localhost:8787"
echo "  Router:  http://localhost:8788"
echo ""
echo "Attach to session with:"
echo -e "  ${BLUE}tmux attach -t $SESSION_NAME${NC}"
echo ""
echo "Stop all workers with:"
echo -e "  ${BLUE}./scripts/dev-stop.sh${NC}"
