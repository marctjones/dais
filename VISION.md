# dais.social Vision

## Mission
Build a single-user ActivityPub server that enables complete ownership of social media presence, independent of platforms, running entirely on Cloudflare's free tier.

## Primary Goals
- **Full Federation**: Seamless interaction with Mastodon, Pleroma, and all ActivityPub networks
- **Zero Cost**: 100% free tier Cloudflare infrastructure (Workers, D1, R2, KV, Pages)
- **Self-Sovereignty**: Complete control over identity, content, and social graph
- **Simplicity**: Easy setup and management via Python CLI

## Target Identity
Primary: `@marc@dais.social`

## Technical Philosophy
- **Rust for Workers**: Performance-critical endpoints compiled to WASM
- **Python for CLI**: Developer-friendly management and testing tools
- **Cloudflare Native**: Leverage edge computing, distributed SQLite (D1), object storage (R2)
- **Standards Compliant**: Full ActivityPub, WebFinger, HTTP Signatures implementation

## Success Criteria
1. Successfully follow/be followed by major Fediverse instances
2. Publish posts visible across the network
3. Receive and display replies, likes, boosts
4. Zero infrastructure costs
5. Simple CLI-based content management

## Non-Goals
- Multi-user support (single-user only)
- Rich web UI (CLI-first, basic landing page only)
- Custom algorithms or timelines (federated content only)
- Commercial features or monetization