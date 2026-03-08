#!/usr/bin/env bash
set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Seeding containerized D1 database...${NC}"

# Check if docker-compose is running
if ! docker-compose ps | grep -q "Up"; then
    echo "Error: Containers not running. Start with: docker-compose up -d"
    exit 1
fi

# Run seed commands in the CLI container
docker-compose exec cli bash -c '
    cd /app/workers/actor

    # Read test keys
    PUBLIC_KEY=$(cat /app/cli/test_keys/public.pem)
    PRIVATE_KEY=$(cat /app/cli/test_keys/private.pem)

    echo "Inserting test actor (marc@localhost)..."
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
        \"https://localhost/users/marc\",
        \"marc\",
        \"Marc (Test User)\",
        \"Container-based test account for dais ActivityPub server\",
        \"${PUBLIC_KEY}\",
        \"${PRIVATE_KEY}\",
        \"https://localhost/users/marc/inbox\",
        \"https://localhost/users/marc/outbox\",
        \"https://localhost/users/marc/followers\",
        \"https://localhost/users/marc/following\"
    );
    "

    echo "Inserting sample followers..."
    wrangler d1 execute DB --local --command="
    INSERT OR REPLACE INTO followers (id, actor_id, follower_actor_id, follower_inbox, status)
    VALUES (\"follow-alice-001\", \"https://localhost/users/marc\", \"https://mastodon.social/users/alice\", \"https://mastodon.social/users/alice/inbox\", \"approved\");

    INSERT OR REPLACE INTO followers (id, actor_id, follower_actor_id, follower_inbox, status)
    VALUES (\"follow-bob-001\", \"https://localhost/users/marc\", \"https://pleroma.example.com/users/bob\", \"https://pleroma.example.com/users/bob/inbox\", \"approved\");

    INSERT OR REPLACE INTO followers (id, actor_id, follower_actor_id, follower_inbox, status)
    VALUES (\"follow-charlie-001\", \"https://localhost/users/marc\", \"https://pixelfed.social/users/charlie\", \"https://pixelfed.social/users/charlie/inbox\", \"pending\");

    INSERT OR REPLACE INTO followers (id, actor_id, follower_actor_id, follower_inbox, status)
    VALUES (\"follow-dave-001\", \"https://localhost/users/marc\", \"https://mastodon.example.com/users/dave\", \"https://mastodon.example.com/users/dave/inbox\", \"rejected\");
    "

    echo "Inserting sample post..."
    wrangler d1 execute DB --local --command="
    INSERT OR REPLACE INTO posts (id, actor_id, content, content_html, visibility, published_at)
    VALUES (\"https://localhost/users/marc/posts/001\", \"https://localhost/users/marc\", \"Hello from containerized dais!\", \"<p>Hello from containerized dais!</p>\", \"public\", datetime(\"now\"));
    "
'

echo -e "${GREEN}✓ Database seeded successfully!${NC}"
echo ""
echo "Summary:"
echo "  • Actor: marc@localhost"
echo "  • Approved followers: 2 (alice, bob)"
echo "  • Pending followers: 1 (charlie)"
echo "  • Rejected followers: 1 (dave)"
echo "  • Sample posts: 1"
