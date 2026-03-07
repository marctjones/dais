# dais - Single-User ActivityPub Server

## Mission
Build a **generic** single-user ActivityPub server that enables complete ownership of social media presence, independent of platforms, running entirely on Cloudflare's free tier.

## Primary Use Cases

### 1. Personal Identity
- Own your personal social media presence
- Handle matches email: `@yourname@yourdomain.com`
- Complements personal homepage/portfolio
- Example: `@alice@alicesmith.com`

### 2. Business/Organization Presence
- Give businesses a Fediverse identity
- Generic handle: `@social@businessdomain.com`
- No changes to existing website
- Professional social media presence independent of platforms
- Example: `@social@mybusiness.com`

## Example Deployment

**The dais project itself** uses dais at `@social@dais.social` for project updates and announcements (dogfooding).

This demonstrates the business/organization pattern:
- Project homepage: `https://dais.social`
- ActivityPub endpoints: `https://social.dais.social`
- Project identity: `@social@dais.social`

## Primary Goals

- **Full Federation**: Seamless interaction with Mastodon, Pleroma, and all ActivityPub networks
- **Zero Cost**: 100% free tier Cloudflare infrastructure (Workers, D1, R2, KV, Pages)
- **Self-Sovereignty**: Complete control over identity, content, and social graph
- **Simplicity**: Easy setup and management via Python CLI
- **Generic & Reusable**: Deploy to ANY Cloudflare account for ANY domain

## Technical Philosophy

- **Rust for Workers**: Performance-critical endpoints compiled to WASM
- **Python for CLI**: Developer-friendly management and testing tools
- **Cloudflare Native**: Leverage edge computing, distributed SQLite (D1), object storage (R2)
- **Standards Compliant**: Full ActivityPub, WebFinger, HTTP Signatures implementation
- **Zero Hardcoding**: All configuration from files - works for any domain/business

## Target Users

- Individuals with custom domains
- Small businesses wanting social media ownership
- Organizations with existing websites
- Anyone wanting email-matching social handles
- Tech enthusiasts who value data sovereignty

## Success Criteria

1. Successfully follow/be followed by major Fediverse instances
2. Publish posts visible across the network
3. Receive and display replies, likes, boosts
4. Zero infrastructure costs (Cloudflare free tier)
5. Simple CLI-based content management
6. One-command deployment to any domain

## Non-Goals

- Multi-user support (single-user only)
- Rich web UI (CLI-first, basic landing page only)
- Custom algorithms or timelines (federated content only)
- Commercial features or monetization
- Platform lock-in (generic, self-hostable)