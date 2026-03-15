# Dais User Guide

**Version:** 1.0
**Last Updated:** March 14, 2026

## Table of Contents

1. [Introduction](#introduction)
2. [Getting Started](#getting-started)
3. [Creating Posts](#creating-posts)
4. [Managing Followers](#managing-followers)
5. [Bluesky Chat](#bluesky-chat)
6. [Direct Messages (ActivityPub)](#direct-messages-activitypub)
7. [Moderation](#moderation)
8. [TUI Reference](#tui-reference)
9. [Privacy & Security](#privacy--security)
10. [Troubleshooting](#troubleshooting)

---

## Introduction

**Dais** is a self-hosted dual-protocol social media server that bridges ActivityPub (Mastodon, Pleroma, etc.) and AT Protocol (Bluesky).

### Key Features

- ✅ **Dual-Protocol Posting** - One post appears on both networks
- ✅ **Privacy Protection** - Automatic privacy checks prevent leaking private posts
- ✅ **Unified Threads** - See replies from both protocols in one view
- ✅ **Bluesky Chats** - Separate messaging system from ActivityPub DMs
- ✅ **Comprehensive Moderation** - Review and filter replies
- ✅ **Self-Hosted** - Full control over your data

---

## Getting Started

### Prerequisites

- Domain name (e.g., `dais.social`)
- Cloudflare account (for Workers, D1, R2)
- Python 3.11+ installed

### Installation

```bash
# Install dais CLI
pip install -e cli/

# Initialize configuration
dais setup init
# Enter: username, domain, Cloudflare credentials

# Deploy to production
dais deploy all

# Verify deployment
dais doctor
```

### First Steps

1. **Create your first post:**
   ```bash
   dais post create "Hello from both worlds! 🌐" --protocol both
   ```

2. **Start the Bluesky reply consumer:**
   ```bash
   tmux new-session -d -s bluesky-consumer "cd services && python bluesky_reply_consumer.py --remote"
   ```

3. **Launch the TUI:**
   ```bash
   dais tui
   ```

---

## Creating Posts

### CLI Commands

**Create a public post (both protocols):**
```bash
dais post create "Hello world!" --protocol both --visibility public
```

**Create followers-only post (ActivityPub only):**
```bash
dais post create "For followers only" --protocol both --visibility followers
```
*Note: Privacy protection automatically blocks Bluesky delivery for non-public posts*

**Create direct message:**
```bash
dais post create "Secret message" --protocol activitypub --visibility direct
```

### Protocol Options

- `--protocol both` - Post to ActivityPub + Bluesky
- `--protocol activitypub` - ActivityPub only
- `--protocol atproto` - Bluesky only

### Visibility Options

- `--visibility public` - Anyone can see (default)
- `--visibility unlisted` - Not in public timelines
- `--visibility followers` - Followers only (ActivityPub only)
- `--visibility direct` - Direct message (ActivityPub only)

### Privacy Protection

**Automatic Safety Checks:**
- Followers-only posts → Blocked from Bluesky (no followers-only support)
- Direct posts → Blocked from Bluesky (privacy protection)
- Warning message shown when protocol is downgraded

**Example:**
```bash
$ dais post create "Private thoughts" --protocol both --visibility followers

⚠️  Privacy Notice: Bluesky doesn't support 'followers' visibility
   All Bluesky posts are public. Posting to ActivityPub only to protect your privacy.
```

---

## Managing Followers

### View Followers

**CLI:**
```bash
# List all followers
dais followers list

# Show follower details
dais followers show @user@mastodon.social
```

**TUI:**
- Press `f` to open Followers screen
- Use arrow keys to navigate
- Press Enter to view follower details

### Approve/Reject Followers

```bash
# Approve follower request
dais followers approve @user@mastodon.social

# Reject follower request
dais followers reject @user@mastodon.social
```

**Note:** Manual approval is enabled by default (`manuallyApprovesFollowers: true`)

---

## Bluesky Chat

**Protocol:** `chat.bsky.convo.*` (separate from ActivityPub DMs)

### Viewing Chats

**TUI:**
1. Press `x` from dashboard
2. Select conversation from list
3. View message history

### Sending Messages

1. Select a conversation
2. Type message in input area
3. Press `Ctrl+S` to send
4. Message appears immediately after sending

### Creating New Chats

**CLI (future):**
```bash
dais chat start alice.bsky.social
```

**TUI:**
- Press `n` to start new chat
- Enter DID or handle (e.g., `alice.bsky.social` or `did:plc:abc123`)
- Conversation created automatically

### Protocol Switching

Press `a` in Bluesky Chats to switch to ActivityPub DMs
Press `b` in ActivityPub DMs to switch to Bluesky Chats

---

## Direct Messages (ActivityPub)

**Protocol:** ActivityPub `Note` with `visibility: direct`

### Sending DMs

```bash
dais dm send @user@mastodon.social "Hello!"
```

### Viewing DMs

**TUI:**
1. Press `i` from dashboard
2. View DM list
3. Press Enter to read conversation

---

## Moderation

### Reviewing Replies

**TUI:**
1. Press `m` to open Moderation screen
2. Filter by status: `pending`, `approved`, `rejected`
3. Review reply content and author
4. Press `a` to approve or `r` to reject

**Moderation Statuses:**
- `pending` - Awaiting review (default)
- `approved` - Visible in threads
- `rejected` - Hidden from threads
- `auto_approved` - Passed automated checks

### Blocking Users

**Block a user:**
```bash
dais block user @spammer@example.com
```

**Block entire domain:**
```bash
dais block domain evil.com
```

**List blocks:**
```bash
dais block list
```

**Unblock:**
```bash
dais block unblock @user@example.com
```

---

## TUI Reference

### Global Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `d` | Dashboard |
| `n` | New Post (Composer) |
| `f` | Followers |
| `m` | Moderation |
| `b` | Blocks |
| `i` | Direct Messages (ActivityPub) |
| `x` | Bluesky Chats |
| `?` | Help |
| `q` | Quit |

### Dashboard Screen

- View post statistics
- See recent activity
- Navigate to other screens

### Composer Screen

- `Ctrl+S` - Send post
- Tab - Switch between text/options
- `Ctrl+C` - Cancel

### Protocol Selection

- **ActivityPub only** - Federated social networks (Mastodon, etc.)
- **Bluesky (AT Protocol)** - Bluesky network
- **Both** - Dual-protocol (appears on both networks)

### Followers Screen

- `r` - Refresh
- `a` - Approve selected
- `x` - Reject selected
- `Enter` - View details

### Moderation Screen

- `r` - Refresh
- `a` - Approve selected reply
- `x` - Reject selected reply
- `h` - Hide/unhide reply
- `f` - Filter by status (pending/approved/rejected)

### Bluesky Chats Screen

- `r` - Refresh conversations
- `a` - Switch to ActivityPub DMs
- `Escape` - Back
- `Ctrl+S` - Send message (when in input)

---

## Privacy & Security

### Privacy Levels

**Public Posts:**
- ✅ Safe for dual-protocol posting
- Visible to anyone on both networks
- Federated across servers

**Followers-Only Posts:**
- ⚠️ ActivityPub only (automatic)
- Bluesky delivery blocked (privacy protection)
- Only your ActivityPub followers can see

**Direct Messages:**
- ⚠️ ActivityPub only
- Private, encrypted in transit
- Not federated publicly

### Security Best Practices

1. **Use strong PDS password** - Stored in `~/.dais/pds-password.txt`
2. **Enable manual follower approval** - Review requests before accepting
3. **Review moderation queue** - Check replies before they appear
4. **Block suspicious domains** - Prevent spam from entire servers
5. **Monitor consumer logs** - Check Bluesky reply consumer for errors

### Data Storage

- **Local:** Configuration in `~/.dais/config.toml`
- **Cloudflare D1:** Posts, followers, replies, messages
- **Cloudflare R2:** Media files (future)
- **Cloudflare Workers:** Edge compute for federation

---

## Troubleshooting

### Common Issues

**Issue:** "D1 database not found"
```bash
# Solution: Update config with database ID
dais doctor  # Check current status
# Manually update ~/.dais/config.toml with correct database_id
```

**Issue:** "PDS authentication failed"
```bash
# Solution: Check password file
cat ~/.dais/pds-password.txt
# Re-generate password if needed
dais setup init --regenerate-password
```

**Issue:** "Bluesky reply consumer not capturing replies"
```bash
# Solution 1: Check consumer is running
ps aux | grep bluesky_reply_consumer

# Solution 2: Restart consumer
tmux kill-session -t bluesky-consumer
tmux new-session -d -s bluesky-consumer "cd services && python bluesky_reply_consumer.py --remote"

# Solution 3: Check logs
tmux attach -t bluesky-consumer
```

**Issue:** "WebFinger endpoint not responding"
```bash
# Test endpoint
dais test webfinger

# Redeploy workers if needed
dais deploy workers
```

### Health Checks

```bash
# Run full diagnostics
dais doctor

# Expected output:
✓ All checks passed (9/9)
```

### Logs & Debugging

**Consumer logs:**
```bash
tmux attach -t bluesky-consumer
# Press Ctrl+B, D to detach
```

**Worker logs:**
```bash
wrangler tail actor --env production
```

**Database inspection:**
```bash
wrangler d1 execute DB --remote --command "SELECT * FROM posts LIMIT 5"
```

---

## Advanced Topics

### Database Migrations

```bash
# Apply new migrations
dais deploy database

# Create custom migration
# Add .sql file to cli/migrations/
# Run: dais deploy database
```

### Custom Domains

Edit worker routes in `workers/*/wrangler.toml`:
```toml
[[env.production.routes]]
pattern = "social.yourdomain.com/*"
```

### Performance Tuning

**Consumer performance:**
- Default: Processes 100-500 commits/second
- Memory: ~50-100 MB
- CPU: <5% average

**Database performance:**
- D1 automatically scales
- Indexes on post_id, actor_id, published_at
- Query cache enabled

---

## Support & Community

**GitHub:** [anthropics/dais](https://github.com/yourusername/dais)
**Issues:** Report bugs via GitHub Issues
**Docs:** Full documentation in `DEPLOYMENT.md`, `CONTRIBUTING.md`

---

**Happy federating! 🌐**
