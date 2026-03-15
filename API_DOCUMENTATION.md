# Dais API Documentation

**Developer reference for extending and customizing Dais**

---

## Table of Contents

1. [Introduction](#introduction)
2. [CLI Commands API](#cli-commands-api)
3. [TUI Components API](#tui-components-api)
4. [Database Schema](#database-schema)
5. [Worker Endpoints](#worker-endpoints)
6. [Configuration API](#configuration-api)
7. [Extension Points](#extension-points)

---

## Introduction

Dais is built with extensibility in mind. This document covers:
- CLI command structure
- TUI widget system
- Database schema
- Worker endpoints
- Configuration options
- How to add custom features

### Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                   User Interface                     │
│  ┌──────────────┐         ┌──────────────┐         │
│  │  CLI (Click) │         │ TUI (Textual)│         │
│  └──────┬───────┘         └──────┬───────┘         │
│         │                        │                  │
│         └────────┬───────────────┘                  │
└──────────────────┼──────────────────────────────────┘
                   │
┌──────────────────┼──────────────────────────────────┐
│               Core Logic                             │
│  ┌──────────────┴───────────────┐                   │
│  │  queue_delivery.py           │                   │
│  │  delivery.py                 │                   │
│  │  config.py                   │                   │
│  └──────────────┬───────────────┘                   │
└─────────────────┼────────────────────────────────────┘
                  │
┌─────────────────┼────────────────────────────────────┐
│            Cloudflare Workers (Rust/WASM)            │
│  ┌─────────┐ ┌─────────┐ ┌────────┐ ┌──────────┐  │
│  │WebFinger│ │  Actor  │ │ Inbox  │ │  Outbox  │  │
│  └────┬────┘ └────┬────┘ └───┬────┘ └────┬─────┘  │
│       │           │           │           │         │
│       └───────────┴───────────┴───────────┘         │
└─────────────────────┬──────────────────────────────┘
                      │
┌─────────────────────┼─────────────────────────────────┐
│            Cloudflare D1 (SQLite)                     │
│  posts │ followers │ replies │ notifications │ ...   │
└───────────────────────────────────────────────────────┘
```

---

## CLI Commands API

### Creating a New Command

**File:** `cli/dais_cli/commands/your_command.py`

```python
import click
from dais_cli.config import Config

@click.command()
@click.option('--remote', is_flag=True, help='Use production database')
@click.argument('message')
def your_command(message: str, remote: bool):
    """Your command description."""
    config = Config()
    config.load()

    # Your logic here
    print(f"Message: {message}")
    print(f"Remote: {remote}")

# Register in cli/dais_cli/cli.py:
# from dais_cli.commands.your_command import your_command
# cli.add_command(your_command)
```

### Available CLI Modules

| Module | Purpose | Key Functions |
|--------|---------|---------------|
| `config.py` | Configuration management | `Config.load()`, `Config.get()`, `Config.set()` |
| `delivery.py` | ActivityPub delivery | `deliver_to_followers()`, `sign_request()` |
| `queue_delivery.py` | Dual-protocol delivery | `deliver_dual_protocol_post()`, `deliver_to_bluesky()` |

### Database Access Pattern

```python
import subprocess
import json
from pathlib import Path

def query_database(query: str, remote: bool = False) -> list:
    """Execute D1 query and return results."""
    project_root = Path(__file__).parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    cmd = ["wrangler", "d1", "execute", "DB", "--command", query]
    if remote:
        cmd.append("--remote")
    else:
        cmd.append("--local")

    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        check=True,
        cwd=str(worker_dir)
    )

    # Parse JSON from wrangler output
    start = result.stdout.find('[')
    end = result.stdout.rfind(']') + 1
    if start >= 0 and end > 0:
        data = json.loads(result.stdout[start:end])
        if data and len(data) > 0 and "results" in data[0]:
            return data[0]["results"]

    return []
```

---

## TUI Components API

### Creating a New Screen

**File:** `cli/dais_cli/tui/screens/your_screen.py`

```python
from textual.app import ComposeResult
from textual.containers import Container
from textual.screen import Screen
from textual.widgets import Static, ListView, ListItem
from textual.binding import Binding

class YourScreen(Screen):
    """Your screen description."""

    CSS = """
    YourScreen {
        background: $surface;
    }

    #title {
        color: $accent;
        text-style: bold;
        padding: 1;
    }
    """

    BINDINGS = [
        Binding("r", "refresh", "Refresh", show=True),
        Binding("escape", "app.pop_screen", "Back", show=True),
    ]

    def compose(self) -> ComposeResult:
        """Compose the UI."""
        yield Static("📋 Your Screen Title", id="title")

        with Container():
            yield ListView(
                ListItem(Static("Item 1")),
                ListItem(Static("Item 2")),
            )

    def action_refresh(self) -> None:
        """Refresh screen data."""
        self.notify("Refreshing...", severity="information")
        # Load data from database
```

### Registering the Screen

**File:** `cli/dais_cli/tui/app.py`

```python
# Add import
from dais_cli.tui.screens.your_screen import YourScreen

# Add binding
BINDINGS = [
    # ... existing bindings ...
    Binding("y", "show_your_screen", "Your Screen", show=True),
]

# Add action
def action_show_your_screen(self) -> None:
    """Show your custom screen."""
    self.push_screen(YourScreen())
```

### Creating Custom Widgets

**File:** `cli/dais_cli/tui/widgets/your_widget.py`

```python
from textual.widget import Widget
from textual.reactive import reactive

class YourWidget(Widget):
    """Your custom widget."""

    # Reactive properties update UI automatically
    count: reactive[int] = reactive(0)

    def render(self) -> str:
        """Render the widget."""
        return f"Count: {self.count}"

    def increment(self) -> None:
        """Increment the count."""
        self.count += 1
```

### Modal Dialogs

Use the `InputModal` for text input:

```python
from dais_cli.tui.widgets.input_modal import InputModal

def show_input_modal(self) -> None:
    """Show input modal."""
    def handle_result(value: str | None) -> None:
        if value:
            self.notify(f"You entered: {value}")

    self.app.push_screen(
        InputModal(
            title="Enter Name",
            description="Please enter your name",
            placeholder="John Doe",
            default_value="",
        ),
        handle_result
    )
```

---

## Database Schema

### Core Tables

#### posts

```sql
CREATE TABLE posts (
    id TEXT PRIMARY KEY,                    -- ActivityPub ID (URL)
    content TEXT NOT NULL,                  -- Post text content
    actor_id TEXT NOT NULL,                 -- Actor ID (always local actor)
    actor_username TEXT NOT NULL,           -- Username
    published_at TEXT NOT NULL,             -- ISO 8601 timestamp
    in_reply_to TEXT,                       -- Parent post ID (if reply)
    visibility TEXT NOT NULL DEFAULT 'public', -- public, unlisted, followers, direct
    protocol TEXT NOT NULL DEFAULT 'activitypub', -- activitypub, atproto, both
    atproto_uri TEXT,                       -- AT Protocol URI
    atproto_cid TEXT,                       -- AT Protocol CID
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

#### followers

```sql
CREATE TABLE followers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    actor_id TEXT NOT NULL,                 -- Follower's ActivityPub ID
    actor_username TEXT NOT NULL,           -- Follower's handle
    inbox_url TEXT NOT NULL,                -- Follower's inbox URL
    shared_inbox_url TEXT,                  -- Follower's shared inbox (optional)
    status TEXT NOT NULL DEFAULT 'pending', -- pending, accepted, rejected
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(actor_id)
);
```

#### replies

```sql
CREATE TABLE replies (
    id TEXT PRIMARY KEY,                    -- Reply ID (ActivityPub or AT Protocol URI)
    post_id TEXT NOT NULL,                  -- Parent post ID
    actor_id TEXT NOT NULL,                 -- Replier's ID
    actor_username TEXT NOT NULL,           -- Replier's handle
    content TEXT NOT NULL,                  -- Reply text
    published_at TEXT NOT NULL,             -- ISO 8601 timestamp
    moderation_status TEXT NOT NULL DEFAULT 'pending', -- pending, approved, rejected
    moderation_score REAL NOT NULL DEFAULT 0.0,
    hidden INTEGER NOT NULL DEFAULT 0,      -- 0 = visible, 1 = hidden
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (post_id) REFERENCES posts(id)
);
```

#### bluesky_conversations

```sql
CREATE TABLE bluesky_conversations (
    id TEXT PRIMARY KEY,                    -- Conversation ID
    participants TEXT NOT NULL,             -- JSON array of DIDs
    last_message_at TEXT,                   -- ISO 8601 timestamp
    last_message_text TEXT,                 -- Preview text
    unread_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

#### bluesky_messages

```sql
CREATE TABLE bluesky_messages (
    id TEXT PRIMARY KEY,                    -- Message ID
    conversation_id TEXT NOT NULL,          -- Conversation ID
    sender_did TEXT NOT NULL,               -- Sender's DID
    sender_handle TEXT,                     -- Sender's handle (cached)
    text TEXT NOT NULL,                     -- Message text
    sent_at TEXT NOT NULL,                  -- ISO 8601 timestamp
    read INTEGER NOT NULL DEFAULT 0,        -- 0 = unread, 1 = read
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (conversation_id) REFERENCES bluesky_conversations(id)
);
```

### Query Examples

**Get recent posts:**
```sql
SELECT id, content, published_at, protocol
FROM posts
ORDER BY published_at DESC
LIMIT 20;
```

**Get approved replies for a post:**
```sql
SELECT actor_username, content, published_at
FROM replies
WHERE post_id = ? AND moderation_status = 'approved'
ORDER BY published_at ASC;
```

**Get pending follower requests:**
```sql
SELECT actor_username, inbox_url, created_at
FROM followers
WHERE status = 'pending'
ORDER BY created_at ASC;
```

---

## Worker Endpoints

### WebFinger Worker

**URL:** `https://dais.social/.well-known/webfinger`

**Method:** GET

**Parameters:**
- `resource` - Account URI (e.g., `acct:social@dais.social`)

**Response:**
```json
{
  "subject": "acct:social@dais.social",
  "links": [
    {
      "rel": "self",
      "type": "application/activity+json",
      "href": "https://social.dais.social/users/social"
    }
  ]
}
```

### Actor Worker

**URL:** `https://social.dais.social/users/:username`

**Method:** GET

**Headers:**
- `Accept: application/activity+json`

**Response:** ActivityPub Actor object

### Inbox Worker

**URL:** `https://social.dais.social/users/:username/inbox`

**Method:** POST

**Headers:**
- `Content-Type: application/activity+json`
- `Signature: <HTTP Signature>`

**Body:** ActivityPub Activity (Follow, Create, etc.)

### Outbox Worker

**URL:** `https://social.dais.social/users/:username/outbox`

**Method:** GET

**Headers:**
- `Accept: application/activity+json`

**Response:** ActivityPub OrderedCollection of activities

### PDS Worker (AT Protocol)

**Base URL:** `https://pds.dais.social`

**Endpoints:**
- `/xrpc/com.atproto.server.createSession` - Authentication
- `/xrpc/com.atproto.repo.createRecord` - Create post
- `/xrpc/chat.bsky.convo.sendMessage` - Send chat message
- `/xrpc/chat.bsky.convo.createConvo` - Create conversation

---

## Configuration API

### Config File Structure

**Location:** `~/.dais/config.toml`

```toml
[server]
domain = "dais.social"
activitypub_domain = "social.dais.social"
pds_domain = "pds.dais.social"
username = "social"
manually_approves_followers = true

[cloudflare]
account_id = "your-account-id"
account_name = "your-account-name"
api_token = "your-api-token"
d1_database_id = "database-uuid"
d1_database_name = "dais-db"
r2_bucket = "dais-media"

[keys]
private_key_path = "~/.dais/keys/private.pem"
public_key_path = "~/.dais/keys/public.pem"
```

### Config API Usage

```python
from dais_cli.config import Config

# Load config
config = Config()
config.load()

# Get values
domain = config.get("server.domain")
db_id = config.get("cloudflare.d1_database_id")

# Set values (automatically saves)
config.set("server.manually_approves_followers", True)

# Check if key exists
if config.get("cloudflare.api_token"):
    print("API token configured")
```

---

## Extension Points

### 1. Custom CLI Commands

Add commands to handle custom workflows:

```python
# cli/dais_cli/commands/custom.py
import click

@click.command()
def mycustom():
    """My custom command."""
    print("Custom logic here")

# Register in cli.py
from dais_cli.commands.custom import mycustom
cli.add_command(mycustom)
```

### 2. Custom TUI Screens

Add screens for new functionality:

```python
# cli/dais_cli/tui/screens/analytics.py
class AnalyticsScreen(Screen):
    """Show post analytics."""

    def compose(self) -> ComposeResult:
        yield Static("📊 Analytics")
        # Your analytics logic
```

### 3. Custom Moderation Filters

Add custom moderation logic:

```python
# cli/dais_cli/moderation.py
def custom_spam_filter(content: str) -> float:
    """Return spam score (0.0-1.0)."""
    if "spam_keyword" in content.lower():
        return 0.9
    return 0.0

# Use in moderation flow
score = custom_spam_filter(reply_content)
if score > 0.8:
    moderation_status = "rejected"
```

### 4. Custom Workers

Add new Cloudflare Workers:

```rust
// workers/custom/src/lib.rs
use worker::*;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // Your custom worker logic
    Response::ok("Custom worker response")
}
```

### 5. Database Migrations

Add new tables/columns:

```sql
-- cli/migrations/999_custom_feature.sql
CREATE TABLE IF NOT EXISTS custom_data (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    data TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Run with: dais deploy database
```

---

## Development Workflow

### 1. Local Development

```bash
# Start local workers
./scripts/dev-start.sh

# Run with --local flag
dais post create "Test" --protocol both --local

# Test TUI locally
dais tui --local
```

### 2. Testing Changes

```bash
# Run TUI tests
python cli/test_tui.py

# Run integration tests (future)
pytest cli/tests/

# Test worker endpoints
dais test webfinger --local
dais test actor --local
```

### 3. Deployment

```bash
# Deploy workers only
dais deploy workers

# Deploy database migrations
dais deploy database

# Full deployment
dais deploy all

# Verify
dais doctor
```

---

## Examples

### Example 1: Add Custom Post Filter

```python
# cli/dais_cli/commands/post.py

def should_post_be_filtered(content: str) -> bool:
    """Custom filter logic."""
    # Block posts with certain keywords
    blocked_keywords = ["spam", "scam"]
    return any(keyword in content.lower() for keyword in blocked_keywords)

# Use in create command
if should_post_be_filtered(text):
    click.echo("Post blocked by custom filter")
    return
```

### Example 2: Add Analytics Screen

```python
# cli/dais_cli/tui/screens/analytics.py

class AnalyticsScreen(Screen):
    """Post analytics dashboard."""

    def compose(self) -> ComposeResult:
        yield Static("📊 Analytics", id="title")

        # Query database for stats
        stats = self.get_stats()

        yield Static(f"Total Posts: {stats['total_posts']}")
        yield Static(f"Total Replies: {stats['total_replies']}")
        yield Static(f"Total Followers: {stats['total_followers']}")

    def get_stats(self) -> dict:
        """Query database for statistics."""
        # Use database query pattern from above
        total_posts = len(query_database("SELECT id FROM posts"))
        total_replies = len(query_database("SELECT id FROM replies"))
        total_followers = len(query_database("SELECT id FROM followers WHERE status='accepted'"))

        return {
            "total_posts": total_posts,
            "total_replies": total_replies,
            "total_followers": total_followers,
        }
```

### Example 3: Custom Webhook Worker

```rust
// workers/webhook/src/lib.rs

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // Send webhook on new post
    let webhook_url = env.var("WEBHOOK_URL")?.to_string();

    // Post notification to webhook
    let client = reqwest::Client::new();
    client.post(&webhook_url)
        .json(&json!({
            "event": "new_post",
            "post_id": "123",
        }))
        .send()
        .await?;

    Response::ok("Webhook sent")
}
```

---

## Best Practices

### 1. Configuration Management

- Store secrets in `~/.dais/` directory
- Never commit credentials to git
- Use environment variables for sensitive data

### 2. Database Access

- Always escape SQL strings to prevent injection
- Use parameterized queries when possible
- Handle database errors gracefully

### 3. Worker Development

- Keep workers stateless
- Use D1 for persistent data
- Handle errors and return appropriate HTTP status codes
- Log important events for debugging

### 4. TUI Development

- Use reactive properties for UI updates
- Handle errors with user-friendly notifications
- Test keyboard shortcuts don't conflict
- Keep screens focused on single purpose

---

## Support

**Issues:** https://github.com/yourusername/dais/issues
**Discussions:** https://github.com/yourusername/dais/discussions
**Contributing:** See `CONTRIBUTING.md`

---

**Happy coding! 🚀**
