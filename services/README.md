# Dais Services

This directory contains background services that extend dais functionality.

## Bluesky Reply Consumer

**File:** `bluesky_reply_consumer.py`

**Purpose:** Subscribe to the Bluesky relay firehose and capture incoming replies to your dual-protocol posts.

### How It Works

When you post to both ActivityPub and Bluesky (`--protocol both`):

1. Your post gets a dual identity:
   - ActivityPub ID: `https://social.dais.social/users/marc/posts/123`
   - AT Protocol URI: `at://did:web:social.dais.social/app.bsky.feed.post/abc`

2. Users on both networks can reply:
   - Mastodon users reply → ActivityPub inbox → `replies` table
   - Bluesky users reply → **This consumer** → `replies` table

3. Result: Unified thread in TUI with mixed replies!

### Usage

```bash
# Local mode (testing)
cd /home/marc/Projects/dais/services
python bluesky_reply_consumer.py --local

# Production mode
python bluesky_reply_consumer.py --remote
```

### Requirements

```bash
pip install atproto websockets
```

### Current Status

**Implemented:**
- ✅ Connects to Bluesky relay firehose
- ✅ Loads your AT Protocol posts from database
- ✅ Database integration (stores replies)
- ✅ Stats tracking

**TODO:**
- ⚠️ Proper CAR block decoding (currently simplified)
- ⚠️ Record extraction from firehose commits
- ⚠️ DID resolution for actor handles
- ⚠️ Reply parent URI matching logic

### Testing

1. Create a dual-protocol post:
   ```bash
   dais post create "Testing cross-protocol replies!" --protocol both
   ```

2. Start the consumer:
   ```bash
   python bluesky_reply_consumer.py --local
   ```

3. Have someone reply on Bluesky

4. Check the TUI thread viewer to see mixed replies!

### Production Deployment

For production, consider running this as:

1. **Systemd service** (recommended for VPS)
   ```ini
   [Unit]
   Description=Dais Bluesky Reply Consumer
   After=network.target

   [Service]
   Type=simple
   User=dais
   WorkingDirectory=/home/dais/dais/services
   ExecStart=/usr/bin/python3 bluesky_reply_consumer.py --remote
   Restart=always

   [Install]
   WantedBy=multi-user.target
   ```

2. **Cloudflare Worker + Durable Object** (for Cloudflare-native stack)
   - Port this logic to a Worker
   - Use Durable Object for persistent WebSocket connection
   - Already have relay_subscription.rs as template

3. **Docker container** (for portability)
   ```dockerfile
   FROM python:3.11-slim
   WORKDIR /app
   COPY requirements.txt .
   RUN pip install -r requirements.txt
   COPY bluesky_reply_consumer.py .
   CMD ["python", "bluesky_reply_consumer.py", "--remote"]
   ```

### Architecture

```
┌─────────────────────────────────────────┐
│  Bluesky Relay Firehose                 │
│  wss://bsky.network/xrpc/...            │
└──────────────┬──────────────────────────┘
               │ WebSocket stream (CAR format)
               ▼
┌─────────────────────────────────────────┐
│  bluesky_reply_consumer.py              │
│  - Parse firehose commits               │
│  - Extract reply records                │
│  - Match to our posts by atproto_uri    │
└──────────────┬──────────────────────────┘
               │ SQL INSERT
               ▼
┌─────────────────────────────────────────┐
│  D1 Database (replies table)            │
│  - id (AT URI)                          │
│  - post_id (ActivityPub ID)             │
│  - actor_id (Bluesky DID)               │
│  - content                              │
└─────────────────────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│  TUI Thread Viewer                      │
│  Shows mixed replies from both networks │
└─────────────────────────────────────────┘
```

### Debugging

Enable verbose logging:
```python
import logging
logging.basicConfig(level=logging.DEBUG)
```

Check what posts are being monitored:
```bash
wrangler d1 execute DB --local --command \
  "SELECT id, atproto_uri FROM posts WHERE atproto_uri IS NOT NULL;"
```

Verify replies are being stored:
```bash
wrangler d1 execute DB --local --command \
  "SELECT * FROM replies ORDER BY published_at DESC LIMIT 10;"
```
