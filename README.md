# dais - Single-User ActivityPub & AT Protocol Server

A complete single-user social media server supporting both **ActivityPub** (Mastodon, Pleroma, etc.) and **AT Protocol** (Bluesky). Built on Cloudflare's infrastructure with zero hosting costs.

**Live Example**: `@social@dais.social` on Mastodon and `@social.dais.social` on Bluesky

## ✨ Features

### Social Protocols
- ✅ **ActivityPub Federation** - Compatible with Mastodon, Pleroma, Pixelfed, and the entire Fediverse
- ✅ **AT Protocol (Bluesky)** - Full Bluesky integration with lexicon support
- ✅ **WebFinger Discovery** - Be discoverable as `@username@yourdomain.com`
- ✅ **HTTP Signatures** - Cryptographically signed federation requests

### Content & Interactions
- ✅ **Posts** - Create, publish, and delete posts with full federation
- ✅ **Media Attachments** - Images, videos, and files via Cloudflare R2
- ✅ **Replies** - Thread conversations across the Fediverse
- ✅ **Direct Messages** - Private encrypted messaging
- ✅ **Likes & Boosts** - Favorite and share posts
- ✅ **Visibility Controls** - Public, unlisted, followers-only, and direct
- ✅ **Content Warnings** - Sensitive content support

### Management
- ✅ **Terminal UI (TUI)** - Interactive dashboard for monitoring and management
- ✅ **CLI Tools** - Complete command-line interface (`dais` command)
- ✅ **Follower Management** - Approve, reject, and remove followers
- ✅ **Moderation** - Block accounts and instances
- ✅ **Search** - Find posts, users, and content
- ✅ **Notifications** - Track follows, likes, replies, and mentions
- ✅ **Statistics** - Analytics on followers, posts, and engagement

### Security & Authentication
- ✅ **Cloudflare Access** - Enterprise-grade authentication for APIs
- ✅ **Service Tokens** - API access for mobile/desktop apps
- ✅ **Identity Providers** - Google, GitHub, Microsoft, etc.
- ✅ **RSA Key Management** - Secure key generation and storage

### Infrastructure
- ✅ **Deployment Automation** - One-command deployment (`dais deploy all`)
- ✅ **Database Migrations** - Automatic schema management
- ✅ **Backup & Restore** - Complete data backup tools
- ✅ **Health Monitoring** - Endpoint health checks
- ✅ **Zero Cost Hosting** - Runs entirely on Cloudflare free tier

## 🚀 Quick Start

### Prerequisites

- **Cloudflare Account** (free tier works)
- **Domain Name** (managed via Cloudflare DNS)
- **Python 3.10+** (for CLI)
- **Rust & wrangler** (for Workers deployment)

### Installation

#### 1. Clone Repository

```bash
git clone https://github.com/yourusername/dais.git
cd dais
```

#### 2. Install CLI

```bash
cd cli
pip install -e .
```

#### 3. Initialize Configuration

```bash
dais setup init
```

This creates `~/.dais/` with:
- `config.toml` - Server configuration (domain, username, Cloudflare credentials)
- `keys/private.pem` - RSA private key (4096-bit)
- `keys/public.pem` - RSA public key

You'll be prompted for:
- **Domain** (e.g., `yourdomain.com`)
- **Username** (e.g., `social`)
- **Cloudflare Account ID**
- **Cloudflare API Token**

#### 4. Deploy Infrastructure

Create Cloudflare D1 database and R2 bucket:

```bash
dais deploy infrastructure
```

This creates:
- **D1 Database** - `dais-db` (SQLite at the edge)
- **R2 Bucket** - `dais-media` (object storage)

#### 5. Upload Secrets

Upload your private key to Cloudflare Workers:

```bash
dais deploy secrets
```

#### 6. Apply Database Migrations

Initialize database schema:

```bash
dais deploy database
```

#### 7. Deploy Workers

Deploy all Cloudflare Workers:

```bash
dais deploy workers
```

This deploys:
- `webfinger` - WebFinger discovery endpoint
- `actor` - ActivityPub actor profile
- `inbox` - Receive federated activities
- `outbox` - Serve your posts
- `auth` - Cloudflare Access authentication
- `pds` - AT Protocol Personal Data Server
- `delivery-queue` - Background job processing
- `router` - Request routing
- `landing` - Static landing page

#### 8. Verify Deployment

```bash
dais deploy verify
```

Tests:
- ✓ WebFinger responds correctly
- ✓ Actor endpoint is accessible
- ✓ Workers are healthy

### All-in-One Deployment

Deploy everything in one command:

```bash
dais deploy all
```

## 📱 Terminal UI (TUI)

Launch the interactive dashboard:

```bash
dais tui
```

### TUI Features

The TUI provides real-time monitoring and management:

**View 1: Followers**
- See all followers with avatars
- Approve/reject pending follows
- Remove followers
- Block accounts

**View 2: Posts**
- List all your posts
- Create new posts
- Delete posts
- View post details

**View 3: Notifications**
- See follows, likes, replies, mentions
- Real-time updates
- Mark as read

**View 4: Timeline**
- View your home timeline
- See posts from people you follow
- Interact with posts

**View 5: Direct Messages**
- Private conversations
- Send/receive DMs
- Thread view

**View 6: Statistics**
- Follower count
- Post count
- Engagement metrics
- Federation stats

### TUI Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `1-6` | Switch between views |
| `↑/↓` | Navigate lists |
| `Enter` | Select item / Take action |
| `n` | New post (in Posts view) |
| `d` | Delete selected item |
| `r` | Reply to post |
| `l` | Like post |
| `b` | Boost post |
| `q` | Quit TUI |
| `?` | Show help |

See [TUI_SHORTCUTS.md](TUI_SHORTCUTS.md) for complete keyboard reference.

## 🛠️ CLI Usage

### Posts

```bash
# Create a public post
dais post create "Hello, Fediverse!"

# Post with image
dais post create "Check this out!" --attach photo.jpg

# Unlisted post
dais post create "Quiet update" --visibility unlisted

# Followers-only post
dais post create "Private update" --visibility followers

# Direct message
dais post create "@alice@mastodon.social Hey!" --visibility direct

# With content warning
dais post create "Spoilers ahead!" --cw "TV Show Spoilers"

# List all posts
dais post list

# Delete a post
dais post delete <post-id>
```

### Followers

```bash
# List all followers
dais followers list

# Approve a follow request
dais followers approve @alice@mastodon.social

# Reject a follow request
dais followers reject @bob@pleroma.example

# Remove a follower
dais followers remove @charlie@pixelfed.social
```

### Moderation

```bash
# Block an account
dais block add @spammer@bad-instance.com

# Block an entire instance
dais block add bad-instance.com

# List blocks
dais block list

# Unblock
dais block remove @user@instance.com
```

### Search

```bash
# Search for users
dais search users "alice"

# Search for posts
dais search posts "activitypub"

# Search locally
dais search posts "rust" --local
```

### Direct Messages

```bash
# List DM conversations
dais dm list

# View a conversation
dais dm show @alice@mastodon.social

# Send a DM
dais dm send @alice@mastodon.social "Hey, how are you?"
```

### Notifications

```bash
# List all notifications
dais notifications list

# List only mentions
dais notifications list --type mention

# Mark as read
dais notifications mark-read <notification-id>
```

### Authentication

```bash
# Set up Cloudflare Access
dais auth setup

# Create API service token
dais auth create-service-token "My Mobile App"

# List service tokens
dais auth list-service-tokens

# Test authentication
dais auth test

# Show auth status
dais auth status
```

### Statistics

```bash
# Show all stats
dais stats

# Refresh from remote
dais stats --refresh
```

### Database Management

```bash
# Backup database
dais db backup

# Restore from backup
dais db restore backup-2026-03-15.db

# Run migrations
dais db migrate

# Show database info
dais db info
```

### Configuration

```bash
# Show current config
dais config show

# Set configuration value
dais config set server.domain yourdomain.com

# Get configuration value
dais config get server.domain
```

### Testing

```bash
# Test WebFinger endpoint
dais test webfinger

# Test Actor endpoint
dais test actor

# Test federation with another instance
dais test federation @user@mastodon.social
```

### Deployment

```bash
# Deploy everything
dais deploy all

# Deploy only infrastructure (D1, R2)
dais deploy infrastructure

# Deploy only secrets
dais deploy secrets

# Deploy only database migrations
dais deploy database

# Deploy only workers
dais deploy workers

# Verify deployment
dais deploy verify

# Check deployment health
dais doctor
```

## 📚 Documentation

### Getting Started
- [DEPLOYMENT.md](DEPLOYMENT.md) - Complete production deployment guide
- [DEVELOPMENT.md](DEVELOPMENT.md) - Local development environment setup
- [CONTAINER_QUICKSTART.md](CONTAINER_QUICKSTART.md) - Quick start with Docker/Podman

### Configuration
- [DNS_SETUP.md](DNS_SETUP.md) - Custom domain and DNS configuration
- [AUTH_API.md](AUTH_API.md) - Cloudflare Access authentication setup

### Operations
- [OPERATIONAL_RUNBOOK.md](OPERATIONAL_RUNBOOK.md) - Day-to-day operations guide
- [BACKUP_RESTORE.md](BACKUP_RESTORE.md) - Backup and disaster recovery
- [TUI_SHORTCUTS.md](TUI_SHORTCUTS.md) - Complete TUI keyboard reference

### Features & APIs
- [API_DOCUMENTATION.md](API_DOCUMENTATION.md) - REST API reference
- [FEDERATION_GUIDE.md](FEDERATION_GUIDE.md) - ActivityPub federation guide
- [PRIVACY_GUIDE.md](PRIVACY_GUIDE.md) - Privacy and data handling
- [USER_GUIDE.md](USER_GUIDE.md) - End-user guide

### Contributing
- [CONTRIBUTING.md](CONTRIBUTING.md) - How to contribute to dais
- [AGENTS.md](AGENTS.md) - AI agent usage guidelines
- [CLAUDE.md](CLAUDE.md) - Claude AI integration notes

## 🏗️ Architecture

### Technology Stack

**Runtime**: Cloudflare Workers (Rust → WebAssembly)
**Database**: D1 (SQLite at the edge)
**Storage**: R2 (S3-compatible object storage)
**CLI**: Python 3.10+ with Click and Rich
**Frontend**: Static HTML (Cloudflare Pages)

### Project Structure

```
dais/
├── workers/              # Cloudflare Workers (Rust → WASM)
│   ├── webfinger/       # WebFinger discovery
│   ├── actor/           # ActivityPub actor
│   ├── inbox/           # Receive activities
│   ├── outbox/          # Serve posts
│   ├── auth/            # Cloudflare Access auth
│   ├── pds/             # AT Protocol server
│   ├── delivery-queue/  # Background jobs
│   ├── router/          # Request routing
│   └── landing/         # Static landing page
│
├── cli/                 # Python CLI
│   ├── dais_cli/        # CLI implementation
│   │   ├── commands/    # CLI commands
│   │   ├── tui/         # Terminal UI
│   │   └── config.py    # Configuration management
│   └── migrations/      # Database migrations
│
└── scripts/             # Development scripts
    ├── dev-start.sh     # Start local dev environment
    ├── backup.sh        # Backup script
    └── seed-local-db.sh # Seed test data
```

### Deployment Architecture

```
Internet
    ↓
Cloudflare DNS
    ↓
┌─────────────────────────────────────┐
│  Cloudflare Workers (Global Edge)  │
├─────────────────────────────────────┤
│  webfinger → WebFinger discovery    │
│  actor → ActivityPub profile        │
│  inbox → Receive federation         │
│  outbox → Serve posts               │
│  auth → Authentication               │
│  pds → AT Protocol                  │
│  router → Request routing           │
└─────────────────────────────────────┘
         ↓              ↓
    ┌────────┐    ┌──────────┐
    │ D1 DB  │    │ R2 Blob  │
    │(SQLite)│    │ (Media)  │
    └────────┘    └──────────┘
```

## 💰 Cost Breakdown

Running on **Cloudflare's free tier**:

| Service | Free Tier Limit | Cost if Exceeded |
|---------|----------------|------------------|
| Workers | 100,000 requests/day | $0.50/million requests |
| D1 Database | 5GB storage, 5M reads/day | $0.75/GB storage |
| R2 Storage | 10GB storage, 1M reads/month | $0.015/GB storage |
| Pages | Unlimited bandwidth | Free |

**Typical personal use**: $0/month (stays within free tier)
**Heavy use** (10K+ followers): ~$5/month

## 🔒 Security

- **RSA-4096 HTTP Signatures** - All federation requests cryptographically signed
- **Cloudflare Access** - Enterprise authentication with SSO support
- **Content Security Policy** - XSS protection
- **Rate Limiting** - DDoS protection via Cloudflare
- **Input Validation** - All inputs sanitized
- **Secure by Default** - No data stored in browser localStorage

See [PRIVACY_GUIDE.md](PRIVACY_GUIDE.md) for details.

## 🌐 Federation

dais federates with:

- **Mastodon** - Full compatibility
- **Pleroma** - Full compatibility
- **Pixelfed** - Photo sharing
- **PeerTube** - Video federation
- **Misskey** - Japanese Fediverse
- **Bluesky** - Via AT Protocol bridge
- **Any ActivityPub server**

See [FEDERATION_GUIDE.md](FEDERATION_GUIDE.md) for federation details.

## 🐛 Troubleshooting

### Common Issues

**Workers not deploying**:
```bash
# Check wrangler authentication
wrangler whoami

# Re-login if needed
wrangler login
```

**Database migrations failing**:
```bash
# Check database exists
wrangler d1 list

# Manually run migration
wrangler d1 execute dais-db --file=cli/migrations/001_initial.sql --remote
```

**WebFinger not resolving**:
```bash
# Test endpoint directly
dais test webfinger

# Check DNS records
dig +short dais.social
```

**Run full diagnostics**:
```bash
dais doctor
```

## 📄 License

MIT License - See [LICENSE](LICENSE) for details.

## 🤝 Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## 🔗 Resources

### ActivityPub
- [ActivityPub Specification](https://www.w3.org/TR/activitypub/)
- [WebFinger RFC 7033](https://tools.ietf.org/html/rfc7033)
- [HTTP Signatures](https://tools.ietf.org/html/draft-cavage-http-signatures)
- [Mastodon ActivityPub Guide](https://docs.joinmastodon.org/spec/activitypub/)

### AT Protocol
- [AT Protocol Specification](https://atproto.com/)
- [Bluesky API Documentation](https://docs.bsky.app/)
- [Lexicon Schema](https://atproto.com/specs/lexicon)

### Cloudflare
- [Workers Documentation](https://developers.cloudflare.com/workers/)
- [D1 Database](https://developers.cloudflare.com/d1/)
- [R2 Storage](https://developers.cloudflare.com/r2/)
- [Cloudflare Access](https://developers.cloudflare.com/cloudflare-one/applications/)

## 📞 Support

- **Issues**: [GitHub Issues](https://github.com/yourusername/dais/issues)
- **Fediverse**: `@social@dais.social`
- **Bluesky**: `@social.dais.social`

## 🙏 Acknowledgments

Built with:
- [Cloudflare Workers](https://workers.cloudflare.com/) - Serverless compute
- [Rust](https://www.rust-lang.org/) - Systems programming language
- [Python](https://www.python.org/) - CLI scripting
- [Rich](https://github.com/Textualize/rich) - Terminal UI library
- [Click](https://click.palletsprojects.com/) - CLI framework

Inspired by the Fediverse community and the decentralized web movement.
