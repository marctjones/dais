#!/usr/bin/env bash
set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Get the project root directory (parent of scripts/)
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"

WORKER_DIR="$PROJECT_ROOT/workers/actor"
MIGRATION_FILE="$PROJECT_ROOT/cli/migrations/001_initial_schema.sql"
TEST_KEYS_DIR="$PROJECT_ROOT/cli/test_keys"

echo -e "${BLUE}Seeding local D1 database for dais development...${NC}"

# Check if migration file exists
if [ ! -f "$MIGRATION_FILE" ]; then
    echo -e "${RED}Migration file not found: $MIGRATION_FILE${NC}"
    exit 1
fi

# Check if test keys exist
if [ ! -f "$TEST_KEYS_DIR/private.pem" ] || [ ! -f "$TEST_KEYS_DIR/public.pem" ]; then
    echo -e "${RED}Test keys not found in: $TEST_KEYS_DIR${NC}"
    exit 1
fi

cd "$WORKER_DIR"

# Create symlinks so all workers share the same database
echo -e "${GREEN}Step 0: Setting up shared database...${NC}"
for worker in webfinger inbox outbox; do
    if [ -L "$PROJECT_ROOT/workers/$worker/.wrangler" ]; then
        echo "  Symlink already exists for $worker"
    elif [ -d "$PROJECT_ROOT/workers/$worker/.wrangler" ]; then
        echo "  Removing existing .wrangler for $worker"
        rm -rf "$PROJECT_ROOT/workers/$worker/.wrangler"
        ln -s "$PROJECT_ROOT/workers/actor/.wrangler" "$PROJECT_ROOT/workers/$worker/.wrangler"
        echo "  Created symlink for $worker"
    else
        ln -s "$PROJECT_ROOT/workers/actor/.wrangler" "$PROJECT_ROOT/workers/$worker/.wrangler"
        echo "  Created symlink for $worker"
    fi
done

echo -e "${GREEN}Step 1: Running database migration...${NC}"
wrangler d1 execute DB --local --file="$MIGRATION_FILE"

echo -e "${GREEN}Step 2: Reading test keys...${NC}"
PUBLIC_KEY=$(cat "$TEST_KEYS_DIR/public.pem")
PRIVATE_KEY=$(cat "$TEST_KEYS_DIR/private.pem")

echo -e "${GREEN}Step 3: Seeding test actor (marc@localhost)...${NC}"

# Insert test actor
wrangler d1 execute DB --local --command="
INSERT OR REPLACE INTO actors (
    id,
    username,
    display_name,
    summary,
    public_key,
    private_key,
    inbox_url,
    outbox_url,
    followers_url,
    following_url
) VALUES (
    'https://localhost/users/marc',
    'marc',
    'Marc (Test User)',
    'Local development test account for dais ActivityPub server',
    '${PUBLIC_KEY}',
    '${PRIVATE_KEY}',
    'https://localhost/users/marc/inbox',
    'https://localhost/users/marc/outbox',
    'https://localhost/users/marc/followers',
    'https://localhost/users/marc/following'
);
"

echo -e "${GREEN}Step 4: Seeding sample followers...${NC}"

# Insert approved follower (Alice from Mastodon)
wrangler d1 execute DB --local --command="
INSERT OR REPLACE INTO followers (
    id,
    actor_id,
    follower_actor_id,
    follower_inbox,
    status
) VALUES (
    'follow-alice-001',
    'https://localhost/users/marc',
    'https://mastodon.social/users/alice',
    'https://mastodon.social/users/alice/inbox',
    'approved'
);
"

# Insert approved follower (Bob from Pleroma)
wrangler d1 execute DB --local --command="
INSERT OR REPLACE INTO followers (
    id,
    actor_id,
    follower_actor_id,
    follower_inbox,
    status
) VALUES (
    'follow-bob-001',
    'https://localhost/users/marc',
    'https://pleroma.example.com/users/bob',
    'https://pleroma.example.com/users/bob/inbox',
    'approved'
);
"

# Insert pending follower (Charlie from Pixelfed)
wrangler d1 execute DB --local --command="
INSERT OR REPLACE INTO followers (
    id,
    actor_id,
    follower_actor_id,
    follower_inbox,
    status
) VALUES (
    'follow-charlie-001',
    'https://localhost/users/marc',
    'https://pixelfed.social/users/charlie',
    'https://pixelfed.social/users/charlie/inbox',
    'pending'
);
"

# Insert rejected follower (Dave from Mastodon)
wrangler d1 execute DB --local --command="
INSERT OR REPLACE INTO followers (
    id,
    actor_id,
    follower_actor_id,
    follower_inbox,
    status
) VALUES (
    'follow-dave-001',
    'https://localhost/users/marc',
    'https://mastodon.example.com/users/dave',
    'https://mastodon.example.com/users/dave/inbox',
    'rejected'
);
"

echo -e "${GREEN}Step 5: Inserting sample post...${NC}"

# Insert a sample post
wrangler d1 execute DB --local --command="
INSERT OR REPLACE INTO posts (
    id,
    actor_id,
    content,
    content_html,
    visibility,
    published_at
) VALUES (
    'https://localhost/users/marc/posts/001',
    'https://localhost/users/marc',
    'Hello from local dais development! This is a test post.',
    '<p>Hello from local dais development! This is a test post.</p>',
    'public',
    datetime('now')
);
"

echo -e "${GREEN}✓ Database seeded successfully!${NC}"
echo ""
echo "Summary:"
echo "  • Actor: marc@localhost"
echo "  • Approved followers: 2 (alice, bob)"
echo "  • Pending followers: 1 (charlie)"
echo "  • Rejected followers: 1 (dave)"
echo "  • Sample posts: 1"
echo ""
echo "Test the setup with:"
echo -e "  ${BLUE}curl 'http://localhost:8787/.well-known/webfinger?resource=acct:marc@localhost'${NC}"
echo -e "  ${BLUE}curl -H 'Accept: application/activity+json' 'http://localhost:8788/users/marc'${NC}"
