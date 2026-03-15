# dais v1.0.0: Stable Cloudflare Edition

🎉 **First stable release!** dais is now production-ready for deploying your own single-user ActivityPub and AT Protocol server on Cloudflare's infrastructure.

## What is dais?

A complete single-user social media server supporting both **ActivityPub** (Mastodon/Fediverse) and **AT Protocol** (Bluesky). Run your own instance at `@you@yourdomain.com` with zero hosting costs on Cloudflare's free tier.

## 🚀 Quick Start

```bash
git clone https://github.com/yourusername/dais.git
cd dais/cli
pip install -e .
dais setup init
dais deploy all
```

See [INSTALL.md](INSTALL.md) for complete installation guide.

## ✨ Major Features

### Social Protocols
- ✅ **ActivityPub Federation** - Full Mastodon, Pleroma, Pixelfed compatibility
- ✅ **AT Protocol (Bluesky)** - Complete Bluesky integration
- ✅ **WebFinger Discovery** - Be found as `@username@domain.com`
- ✅ **HTTP Signatures** - Cryptographically signed requests

### Content & Interactions
- ✅ **Posts** - Create, edit, delete with full federation
- ✅ **Media** - Images and videos via Cloudflare R2
- ✅ **Replies** - Threaded conversations
- ✅ **Direct Messages** - Private encrypted messaging
- ✅ **Likes & Boosts** - Favorite and share posts
- ✅ **Visibility Controls** - Public, unlisted, followers-only, direct

### Management
- ✅ **Terminal UI (TUI)** - Interactive dashboard with 6 views
- ✅ **CLI Tools** - Complete `dais` command-line interface
- ✅ **Follower Management** - Approve, reject, remove followers
- ✅ **Moderation** - Block accounts and instances
- ✅ **Search** - Find posts and users
- ✅ **Notifications** - Track all activity in real-time
- ✅ **Statistics** - Analytics on followers, posts, engagement

### Security & Auth
- ✅ **Cloudflare Access** - Enterprise SSO authentication
- ✅ **Service Tokens** - API access for mobile/desktop apps
- ✅ **Identity Providers** - Google, GitHub, Microsoft, etc.
- ✅ **RSA-4096 Keys** - Secure key management

### Infrastructure
- ✅ **One-Command Deploy** - `dais deploy all`
- ✅ **Database Migrations** - Automatic schema management
- ✅ **Backup & Restore** - Complete data backup
- ✅ **Health Monitoring** - Endpoint checks
- ✅ **Zero Cost** - Runs on Cloudflare free tier

## 📊 Stats

- **200+ Features** implemented
- **9 Cloudflare Workers** (Rust → WebAssembly)
- **14 CLI Command Groups**
- **6 TUI Views** with real-time updates
- **13,000+ Lines of Code**
- **12 Documentation Guides**

## 🛠️ Technology Stack

- **Runtime**: Cloudflare Workers (Rust → WASM)
- **Database**: Cloudflare D1 (SQLite at edge)
- **Storage**: Cloudflare R2 (S3-compatible)
- **CLI**: Python 3.10+ with Click + Rich
- **Cost**: $0/month (free tier), ~$5/month for heavy use

## 📚 Documentation

### Quick Start
- [README.md](README.md) - Overview and features
- [INSTALL.md](INSTALL.md) - Installation guide
- [FEATURES.md](FEATURES.md) - Complete feature list (200+)
- [CHANGELOG.md](CHANGELOG.md) - Version history

### Deployment
- [DEPLOYMENT.md](DEPLOYMENT.md) - Production deployment
- [DNS_SETUP.md](DNS_SETUP.md) - DNS configuration
- [AUTH_API.md](AUTH_API.md) - Authentication setup

### Operations
- [OPERATIONAL_RUNBOOK.md](OPERATIONAL_RUNBOOK.md) - Day-to-day ops
- [BACKUP_RESTORE.md](BACKUP_RESTORE.md) - Backup procedures
- [USER_GUIDE.md](USER_GUIDE.md) - End-user guide

### Reference
- [API_DOCUMENTATION.md](API_DOCUMENTATION.md) - REST API
- [FEDERATION_GUIDE.md](FEDERATION_GUIDE.md) - ActivityPub details
- [TUI_SHORTCUTS.md](TUI_SHORTCUTS.md) - Keyboard shortcuts
- [PRIVACY_GUIDE.md](PRIVACY_GUIDE.md) - Privacy policy

### Development
- [DEVELOPMENT.md](DEVELOPMENT.md) - Dev environment
- [CONTAINER_QUICKSTART.md](CONTAINER_QUICKSTART.md) - Docker/Podman
- [CONTRIBUTING.md](CONTRIBUTING.md) - Contributing guide

## 🎯 Who is this for?

- **Technical users** who want to run their own social media
- **Privacy-conscious users** who want full data ownership
- **Content creators** who want independence from platforms
- **Developers** interested in ActivityPub or AT Protocol
- **Anyone** tired of centralized social media

## 💰 Cost

Cloudflare free tier includes:
- 100,000 Worker requests/day
- 5GB D1 storage
- 10GB R2 storage
- Unlimited bandwidth

**Typical personal use**: $0/month
**Heavy use (10K+ followers)**: ~$5/month

## 🔒 Security

- RSA-4096 HTTP signatures
- Cloudflare Access authentication
- Rate limiting and DDoS protection
- No tracking or analytics cookies
- GDPR-compliant privacy design
- Data export and deletion support

## 🌐 Federation

Works with all ActivityPub servers:
- **Mastodon** ✅
- **Pleroma** ✅
- **Pixelfed** ✅
- **PeerTube** ✅
- **Misskey** ✅
- **Bluesky** ✅ (via AT Protocol)

## 🚦 What's Next?

### Coming in v1.x
- Bug fixes and polish
- Performance improvements
- Additional IdP support
- Enhanced TUI features

### Planned for v2.0 (Multi-Platform)
- **Vercel** deployment support
- **Netlify Edge** deployment support
- **Deno Deploy** support
- Platform abstraction layer
- Unified multi-platform CLI

## 📦 Installation

### Prerequisites
- Cloudflare account (free)
- Domain name (managed by Cloudflare)
- Python 3.10+
- Rust + wrangler CLI

### Quick Install
```bash
# Clone repository
git clone https://github.com/yourusername/dais.git
cd dais

# Install CLI
cd cli && pip install -e .

# Initialize and deploy
dais setup init
dais deploy all
```

See [INSTALL.md](INSTALL.md) for complete instructions.

## 🐛 Troubleshooting

### Common Issues

**Workers not deploying?**
```bash
wrangler login
dais deploy workers
```

**Database issues?**
```bash
dais deploy infrastructure
dais deploy database
```

**Need diagnostics?**
```bash
dais doctor
```

See [OPERATIONAL_RUNBOOK.md](OPERATIONAL_RUNBOOK.md) for troubleshooting guide.

## 📞 Support

- **Documentation**: Comprehensive guides included
- **Issues**: [GitHub Issues](https://github.com/yourusername/dais/issues)
- **Fediverse**: `@social@dais.social`
- **Bluesky**: `@social.dais.social`

## 🙏 Acknowledgments

Built with:
- [Cloudflare Workers](https://workers.cloudflare.com/)
- [Rust](https://www.rust-lang.org/)
- [Python](https://www.python.org/)
- [Rich](https://github.com/Textualize/rich)
- [Click](https://click.palletsprojects.com/)

Special thanks to the Fediverse community and the decentralized web movement.

## 📄 License

MIT License - See [LICENSE](LICENSE)

## 🤝 Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md)

---

**Full Changelog**: https://github.com/yourusername/dais/blob/main/CHANGELOG.md

**Assets**: Install with `pip install -e .` from the `cli/` directory
