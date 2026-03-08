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

echo -e "${GREEN}Project root: $PROJECT_ROOT${NC}"

# Create new tmux session in detached mode
tmux new-session -d -s "$SESSION_NAME" -c "$PROJECT_ROOT"

# Window 0: WebFinger worker (port 8787)
tmux rename-window -t "$SESSION_NAME:0" "webfinger"
tmux send-keys -t "$SESSION_NAME:0" "cd $PROJECT_ROOT/workers/webfinger" C-m
tmux send-keys -t "$SESSION_NAME:0" "echo -e '${GREEN}Starting WebFinger worker on port 8787...${NC}'" C-m
tmux send-keys -t "$SESSION_NAME:0" "wrangler dev --local --port 8787" C-m

# Window 1: Actor worker (port 8788)
tmux new-window -t "$SESSION_NAME:1" -n "actor" -c "$PROJECT_ROOT/workers/actor"
tmux send-keys -t "$SESSION_NAME:1" "echo -e '${GREEN}Starting Actor worker on port 8788...${NC}'" C-m
tmux send-keys -t "$SESSION_NAME:1" "wrangler dev --local --port 8788" C-m

# Window 2: Inbox worker (port 8789)
tmux new-window -t "$SESSION_NAME:2" -n "inbox" -c "$PROJECT_ROOT/workers/inbox"
tmux send-keys -t "$SESSION_NAME:2" "echo -e '${GREEN}Starting Inbox worker on port 8789...${NC}'" C-m
tmux send-keys -t "$SESSION_NAME:2" "wrangler dev --local --port 8789" C-m

# Window 3: Outbox worker (port 8790)
tmux new-window -t "$SESSION_NAME:3" -n "outbox" -c "$PROJECT_ROOT/workers/outbox"
tmux send-keys -t "$SESSION_NAME:3" "echo -e '${GREEN}Starting Outbox worker on port 8790...${NC}'" C-m
tmux send-keys -t "$SESSION_NAME:3" "wrangler dev --local --port 8790" C-m

# Window 4: Shell for running commands
tmux new-window -t "$SESSION_NAME:4" -n "shell" -c "$PROJECT_ROOT"
tmux send-keys -t "$SESSION_NAME:4" "echo -e '${BLUE}Welcome to dais development shell${NC}'" C-m
tmux send-keys -t "$SESSION_NAME:4" "echo -e 'Workers running on:'" C-m
tmux send-keys -t "$SESSION_NAME:4" "echo -e '  WebFinger: http://localhost:8787'" C-m
tmux send-keys -t "$SESSION_NAME:4" "echo -e '  Actor:     http://localhost:8788'" C-m
tmux send-keys -t "$SESSION_NAME:4" "echo -e '  Inbox:     http://localhost:8789'" C-m
tmux send-keys -t "$SESSION_NAME:4" "echo -e '  Outbox:    http://localhost:8790'" C-m
tmux send-keys -t "$SESSION_NAME:4" "echo ''" C-m
tmux send-keys -t "$SESSION_NAME:4" "echo -e 'Run ${GREEN}./scripts/seed-local-db.sh${NC} to seed the database'" C-m
tmux send-keys -t "$SESSION_NAME:4" "echo -e 'Run ${GREEN}./scripts/test-phase1-local.sh${NC} to test Phase 1'" C-m
tmux send-keys -t "$SESSION_NAME:4" "echo -e 'Run ${GREEN}./scripts/test-phase2-local.sh${NC} to test Phase 2'" C-m

# Select the shell window
tmux select-window -t "$SESSION_NAME:4"

echo -e "${GREEN}✓ Tmux session '$SESSION_NAME' created successfully${NC}"
echo ""
echo "Workers starting on:"
echo "  WebFinger: http://localhost:8787"
echo "  Actor:     http://localhost:8788"
echo "  Inbox:     http://localhost:8789"
echo "  Outbox:    http://localhost:8790"
echo ""
echo "Attach to session with:"
echo -e "  ${BLUE}tmux attach -t $SESSION_NAME${NC}"
echo ""
echo "Stop all workers with:"
echo -e "  ${BLUE}./scripts/dev-stop.sh${NC}"
