# Changelog

All notable changes to dais will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-03-15

### Added - Core Protocols
- ActivityPub federation with full Mastodon/Pleroma compatibility
- AT Protocol (Bluesky) integration with PDS server
- WebFinger discovery (`@username@domain.com`)
- HTTP Signatures with RSA-4096 cryptographic signing
- Shared inbox for efficient batch delivery

### Added - Content Features
- Post creation, editing, and deletion with federation
- Media attachments (images, videos) via Cloudflare R2
- Multiple visibility levels (public, unlisted, followers-only, direct)
- Content warnings for sensitive content
- Replies and threaded conversations
- Likes (favorites) and boosts (announces)
- Direct messaging with thread view
- Mentions and hashtags
- Custom emoji support

### Added - Social Features
- Follow request approval/rejection workflow
- Follower and following list management
- Remove followers (soft block)
- Notifications for follows, mentions, replies, likes, boosts, DMs
- Real-time notification updates
- User search across Fediverse
- Post search with full-text search
- Hashtag search
- Advanced search filters

### Added - Moderation
- Block accounts
- Block entire instances
- Mute accounts
- Mute keywords
- Content filtering
- Report content to instance admins

### Added - Management Tools
- **Terminal UI (TUI)** with 6 views:
  - Followers view - manage followers and requests
  - Posts view - create, view, delete posts
  - Notifications view - track all activity
  - Timeline view - home feed
  - Direct Messages view - private conversations
  - Statistics view - analytics dashboard
- Full keyboard navigation in TUI
- Real-time updates in TUI
- Rich formatting with colors and tables
- **CLI commands** for all operations:
  - `dais post` - Post management
  - `dais followers` - Follower management
  - `dais block` - Moderation
  - `dais search` - Search functionality
  - `dais notifications` - Notification management
  - `dais dm` - Direct messaging
  - `dais stats` - Statistics and analytics
  - `dais config` - Configuration management
  - `dais deploy` - Deployment automation
  - `dais db` - Database operations
  - `dais auth` - Authentication setup
  - `dais test` - Testing utilities
  - `dais doctor` - System diagnostics

### Added - Authentication
- Cloudflare Access integration
- Support for multiple identity providers:
  - Google
  - GitHub
  - Microsoft
  - Facebook
  - LinkedIn
  - One-time PIN (email)
- Service tokens for API/automation access
- JWT verification
- Session management
- Multi-factor authentication via IdP

### Added - Deployment & Infrastructure
- One-command deployment (`dais deploy all`)
- Automatic D1 database creation
- Automatic R2 bucket creation
- Secret management and upload
- Database migration automation
- Worker deployment automation
- Health check verification
- **9 Cloudflare Workers**:
  - `webfinger` - WebFinger discovery
  - `actor` - ActivityPub actor profile
  - `inbox` - Receive federated activities
  - `outbox` - Serve posts
  - `auth` - Cloudflare Access authentication
  - `pds` - AT Protocol Personal Data Server
  - `delivery-queue` - Background job processing
  - `router` - Request routing
  - `landing` - Static landing page
- Rust → WebAssembly compilation for performance
- Global edge deployment (300+ locations)

### Added - Database & Storage
- D1 (SQLite) database for relational data
- R2 object storage for media
- Cloudflare Queues for async jobs
- Durable Objects for WebSocket state
- Database migrations with versioning
- Full-text search support
- Automatic database replication
- S3-compatible R2 API

### Added - Backup & Recovery
- Database backup (`dais db backup`)
- Database restore (`dais db restore`)
- Media backup support
- Point-in-time recovery
- Scheduled backup scripts
- Backup verification

### Added - Monitoring & Analytics
- Follower count and statistics
- Post count and engagement metrics
- Media usage statistics
- Federation statistics
- Storage usage tracking
- Endpoint health checks
- Worker status monitoring
- Response time tracking
- Error rate monitoring
- `dais doctor` diagnostic command

### Added - Security
- RSA-4096 key generation
- HTTP signature verification
- Cloudflare Access zero-trust authentication
- IP allowlisting
- Geographic restrictions
- Rate limiting per endpoint
- DDoS protection via Cloudflare
- No tracking or analytics cookies
- GDPR-compliant privacy design
- Data export and deletion

### Added - Developer Tools
- Local development environment
- Wrangler dev mode for hot reload
- SQLite local database for testing
- Database seeding scripts
- Tmux development environment
- Unit tests for Rust workers
- Integration tests
- CLI pytest test suite
- Test coverage reporting
- Mock data generation

### Added - Documentation
- README.md with quick start guide
- FEATURES.md with complete feature list
- DEPLOYMENT.md with production setup guide
- DEVELOPMENT.md with dev environment setup
- CONTAINER_QUICKSTART.md for Docker/Podman
- API_DOCUMENTATION.md with REST API reference
- FEDERATION_GUIDE.md with ActivityPub details
- AUTH_API.md with authentication setup
- TUI_SHORTCUTS.md with keyboard reference
- OPERATIONAL_RUNBOOK.md with operations guide
- BACKUP_RESTORE.md with backup procedures
- PRIVACY_GUIDE.md with privacy policy
- USER_GUIDE.md with end-user documentation
- CONTRIBUTING.md with contribution guidelines
- DNS_SETUP.md with DNS configuration

### Technical Details
- **Language**: Rust (Workers) + Python 3.10+ (CLI)
- **Runtime**: Cloudflare Workers (WebAssembly)
- **Database**: Cloudflare D1 (SQLite)
- **Storage**: Cloudflare R2 (S3-compatible)
- **Queue**: Cloudflare Queues
- **State**: Cloudflare Durable Objects
- **CLI Framework**: Click + Rich
- **Cost**: $0/month on free tier, ~$5/month for heavy use

### Breaking Changes
None - this is the initial stable release.

### Migration Notes
- Migrating from v0.x: Run `dais deploy database` to apply all migrations
- Existing keys in `~/.dais/keys/` are preserved
- Configuration format unchanged

---

## [0.1.0] - 2025-xx-xx (Development Versions)

All development work leading to v1.0.0 stable release.

### Development Milestones
- Phase 1: Basic Federation (WebFinger, Actor, Inbox, Followers)
- Phase 2: Content Publishing (Outbox, Posts, Delivery)
- Phase 2.5: Media Attachments (R2 integration, Image/Video upload)
- Phase 3: Interactions (Replies, Likes, Boosts, DMs)
- Phase 4: Management (TUI, Enhanced CLI, Statistics)
- Phase 5: AT Protocol (Bluesky integration, PDS server)
- Phase 6: Authentication (Cloudflare Access, Service Tokens)
- Phase 7: Deployment Automation (One-command deploy)

---

## Release Schedule

- **v1.0.0** (2026-03-15) - Stable release, Cloudflare-only
- **v1.1.0** (TBD) - Bug fixes, minor features
- **v2.0.0** (TBD) - Multi-platform support (Vercel, Netlify, Deno)

## Versioning Strategy

- **Major version** (x.0.0) - Breaking changes, major features
- **Minor version** (1.x.0) - New features, no breaking changes
- **Patch version** (1.0.x) - Bug fixes, security updates

## Support

- **v1.0.x** - Active development and support
- **v0.x** - No longer supported, upgrade to v1.0.0

[1.0.0]: https://github.com/yourusername/dais/releases/tag/v1.0.0
[0.1.0]: https://github.com/yourusername/dais/releases/tag/v0.1.0
