# Mainstream Social Platform Lessons for dais

**Status:** Research synthesis, 2026-06-22
**Purpose:** Document what mainstream users appear to value in major social
platforms, what they criticize, and what dais should copy, reject, or redesign.
This document extends `docs/POSITIONING.md`: dais is a private-by-default,
self-sovereign social home, not a surveillance or engagement-maximization
platform.

## Executive summary

People do not dislike social media as a category. They like seeing friends,
family, creators, news, videos, group conversations, events, recommendations,
private messaging, and niche communities. They object to the dominant business
model and product posture: behavioral advertising, data extraction, addictive
feeds, opaque moderation, harassment, audience collapse, misinformation,
creator/platform dependency, and feeds that feel hard to control.

dais should therefore avoid trying to be a clone of Facebook, X, Instagram,
TikTok, YouTube, Reddit, Discord, Telegram, or LinkedIn. It should provide the
parts people actually want:

- a private friends-and-family social graph;
- public posting when explicitly chosen;
- private and E2EE direct/group communication;
- intentional follows, watches, RSS/news sources, and creator feeds;
- small group/community spaces;
- useful media support;
- calm discovery and search;
- understandable moderation and source context;
- owner-controlled identity, data, and relationships.

And it should reject the exploitative defaults:

- no behavioral ads;
- no engagement-maximizing default feed;
- no public-by-default posting;
- no dark-pattern notification or streak mechanics;
- no selling graph data;
- no hidden audience downgrade between protocols;
- no opaque ranking that the user cannot inspect or disable.

## Evidence base

This is a live product-research document. Re-check sources before making claims
about current platform size, feature availability, or revenue.

- The FTC's 2024 staff report on major social/video platforms found broad
  surveillance, weak data minimization, extensive data retention/sharing,
  targeted-ad incentives, limited user control over algorithmic data use, and
  weak protections for children and teens. The report covered Amazon/Twitch,
  Facebook/Meta, YouTube, Twitter/X, Snap, ByteDance/TikTok, Discord, Reddit,
  and WhatsApp.
  <https://www.ftc.gov/news-events/news/press-releases/2024/09/ftc-staff-report-finds-large-social-media-video-streaming-companies-have-engaged-vast-surveillance>
- Pew's 2025 social media landscape reporting shows fragmentation rather than a
  single replacement platform. YouTube, Facebook, Instagram, TikTok, Reddit,
  WhatsApp, and others each satisfy different user needs.
  <https://www.axios.com/2025/11/28/pew-social-media-reddit-youtube-ai>
- Pew's teen platform reporting shows YouTube, TikTok, Instagram, and Snapchat
  remain central to teen social life, while Facebook and X are less central for
  teens than they were historically.
  <https://apnews.com/article/02defc5b53dc4216da1efa63c82a30af>
- The U.S. Surgeon General's advisory says social media can provide connection
  and support but presents unresolved risks around heavy use, sleep, body image,
  harmful content, harassment, and platform transparency.
  <https://www.hhs.gov/surgeongeneral/reports-and-publications/youth-mental-health/social-media/index.html>
- Pew found Americans who mainly get political news from social media are less
  knowledgeable about current events and more likely to hear unproven claims.
  <https://www.pewresearch.org/journalism/2020/07/30/americans-who-mainly-get-their-news-on-social-media-are-less-engaged-less-knowledgeable/>
- Pew's Twitter/X news research found many users value the platform for breaking
  news, but trust, stress, and democratic impact are politically divided.
  <https://www.pewresearch.org/journalism/2021/11/15/news-on-twitter-consumed-by-most-users-and-trusted-by-many/>
- Pew's online harassment research found social media remains the most common
  venue for online harassment experiences.
  <https://www.pewresearch.org/internet/2021/01/13/the-state-of-online-harassment/>
- Research on TikTok's For You feed finds users may have difficulty steering
  recommendations away from topics they no longer want, especially when controls
  are buried or require repeated effort.
  <https://arxiv.org/abs/2605.10690>
- A Twitter/X timeline audit found algorithmic and chronological feeds produce
  different news exposure patterns; the practical lesson for dais is not
  "algorithm always bad" but "ranking changes civic attention and must be
  inspectable, optional, and user-controlled."
  <https://arxiv.org/abs/2406.17097>
- Research on public support for misinformation interventions found support
  depends most on perceived fairness, then effectiveness, then intrusiveness;
  user-agency and transparency interventions such as labels are generally more
  acceptable than opaque removals.
  <https://arxiv.org/abs/2508.05849>
- Mastodon user research found privacy from data mining and control are major
  reasons people seek alternative social media.
  <https://arxiv.org/abs/2303.01285>
- Telegram research shows the value and risk of large semi-private channels:
  they can support high-scale communities and broadcast, but also host
  conspiratorial, hyper-partisan, illicit, and cybercriminal ecosystems when
  moderation and abuse response are weak.
  <https://arxiv.org/abs/2410.23638>
  <https://arxiv.org/abs/2409.14596>

## What people like by platform

| Platform | What people like | What dais should learn |
| --- | --- | --- |
| Facebook | Real-life social graph, family/friend updates, groups, events, marketplace, local/community continuity across generations. | Copy the feeling of "people I actually know" and groups/events. Reject surveillance ads, public ambiguity, and feed manipulation. |
| Instagram | Visual identity, photos/video, stories, creators, DMs, lightweight social presence, aspirational discovery. | Support polished media posts and stories/albums eventually. Downplay like/follower counts and body-image/status pressure. |
| Threads | Simple public text posting, Meta-scale onboarding, communities, Twitter/X alternative, partial ActivityPub direction. | Public text and federation matter, but dais must not make Meta's public-by-default/algorithmic posture the center. |
| TikTok | High-quality entertainment discovery, creative remix culture, short video, creator reach, easy passive browsing. | Discovery is valuable, but it belongs in an explicit Discovery mode with strong controls, not as the default Home loop. |
| YouTube | Deep video library, learning, long-form creators, subscriptions, search, creator monetization. | Support intentional video/media watching and source subscriptions. Avoid autoplay/rabbit-hole defaults and low-quality AI/media spam. |
| X / Twitter | Breaking news, journalists, public figures, live events, public conversation, fast replies. | Keep public broadcast and replies, but isolate them from private Home. Source context and harassment controls must be stronger. |
| Reddit | Niche communities, pseudonymity, expertise, discussion archives, question answering, moderation by community. | Add watchable communities/sources and topic spaces, but keep data portable and owner-controlled. |
| Snapchat | Close-friend messaging, lightweight photos/video, ephemerality, teen social presence. | DMs and close groups are essential. Ephemeral UX must not imply true safety; use E2EE and clear risk language. |
| Discord | Persistent invite-based communities, voice/video, roles, channels, bots/integrations, small-group belonging. | Dais should support private group spaces, roles, voice/video integration later, and owner/operator controls without becoming a closed silo. |
| Telegram | Fast large groups/channels, broadcast, cross-device messaging, large file/media sharing, perceived privacy/control. | Channels and watchable public feeds are useful. Pair them with abuse controls, source labels, and clear E2EE distinctions. |
| WhatsApp / Signal | Private messaging, family/friend groups, phone-native habit, E2EE expectations. | Dais DMs and private groups must be simple and trustworthy, not protocol-explained. |
| LinkedIn | Professional identity, weak-tie network, hiring, business updates, reputation. | If supported, professional identity should be a separate account/persona, not merged into personal/family social space. |
| Bluesky | Public conversation, starter packs, custom feeds, decentralized identity ideas, gentler X alternative. | Starter packs, custom feeds, and portable identity are worth copying. Private/friends content must not route to public-only protocols. |
| Mastodon / Fediverse | Federation, local moderation, no ads, chronological feeds, content warnings, post visibility controls. | Keep ActivityPub interop and visibility controls, but improve onboarding, search, thread completeness, and user language. |

## Cross-platform user needs

### Real relationships

Facebook, Snapchat, WhatsApp, Discord, and Instagram all show the same durable
need: people want social software for real relationships. Strong ties are
especially valuable for personal networks; weak ties are more useful for public
and professional networks.

dais requirement:

- Home defaults to friends, follows, watches, and chosen sources.
- The friends graph is private by default.
- Relationship state is obvious: follower, following, friend, watched source,
  blocked, muted.
- Private groups and named audience lists are first-class.

### Entertainment and discovery

TikTok, YouTube, Instagram, Reddit, and X show that people like discovery,
novelty, creators, and current events. The problem is not discovery itself; it is
discovery that silently takes over the product.

dais requirement:

- Discovery is explicit, labeled, and escapable.
- "Why am I seeing this?" is available for every recommended item.
- A user can remove a topic/source and make that decision stick.
- No algorithmic recommendations appear in the main Home lane unless the owner
  explicitly enables them.

### News and information

Users value breaking news and first-person reports on X, TikTok, YouTube,
Facebook, Instagram, Reddit, and Telegram. The problem is trust, provenance,
source quality, misinformation, and context collapse.

dais requirement:

- Let users follow official RSS feeds, ActivityPub accounts, Bluesky accounts,
  newsletters, and public Telegram/Discord-style sources when available.
- Show source type, domain, verification/trust signals, and original links.
- Add "read before repost" friction for links and media.
- Support community notes/context labels as advisory overlays, not opaque
  censorship.

### Private messaging and groups

WhatsApp, Signal, Snapchat, Discord, Telegram, Instagram DMs, and Facebook
Messenger show that private messaging is not secondary to social media; it is
one of the main ways people actually socialize.

dais requirement:

- E2EE DMs and small groups are a core product, not an add-on.
- Distinguish "private to followers" from "encrypted to recipients."
- Make replies, DMs, group posts, and audience-scoped posts visually distinct.
- Provide safety affordances for minors/families if dais.cloud ever targets
  family use.

### Communities

Reddit, Discord, Facebook Groups, Telegram channels, LinkedIn groups, and
Threads communities show that people want topic spaces and group identity.

dais requirement:

- Support small private groups and watchable public communities.
- Let the owner decide whether a community is a private audience, a watched
  source, or a public publishing identity.
- Community membership should not be public unless explicitly chosen.

### Creator support

Instagram, TikTok, YouTube, Substack, Patreon, Twitch, Telegram channels, and X
show that creators need publishing, subscriptions, media, audience analytics, and
direct fan relationships.

dais requirement:

- Public creator mode should be supported, but not be the default persona.
- Creator analytics must be privacy-preserving and local-first.
- Paid creator features should not require behavioral ads or platform lock-in.

## What dais should reject

### Behavioral advertising

Behavioral ads are the main economic engine behind surveillance and engagement
optimization. dais should not implement ad targeting based on user behavior,
private graph membership, inferred demographics, sensitive interests, or
cross-site tracking.

Acceptable alternatives:

- no ads;
- contextual sponsorships for public newsletters/feeds only;
- owner-controlled affiliate links with disclosure;
- local directory sponsorships only if the owner opts in.

### Engagement-maximized feeds

Infinite scroll, autoplay, streaks, push-notification pressure, and buried
controls are not neutral UX. They are product choices that shift agency from the
user to the platform.

dais should prefer:

- chronological or rule-based Home;
- "caught up" states;
- daily/weekly digests;
- quiet hours;
- explicit Discovery mode;
- transparent feed rules.

### Public-by-default culture

Most mainstream platforms normalize public or semi-public content. This creates
audience collapse: family, coworkers, strangers, journalists, trolls, and
algorithms all become one implied audience.

dais should prefer:

- followers/friends as the default for personal accounts;
- visible audience indicators on every post and reply;
- explicit confirmation when posting publicly or bridging to Bluesky/public AP;
- protocol capability checks that refuse private-to-public downgrades.

### Opaque moderation

Meta/Threads/Instagram criticism shows that moderation errors are worse when
users receive vague explanations and weak appeals. Telegram shows the opposite
risk: weak moderation can allow harmful networks to thrive.

dais should prefer:

- owner-controlled moderation queues;
- visible reason labels;
- reversible mutes before destructive blocks;
- appeal/review logs for managed hosting;
- AI advisories that explain confidence and evidence;
- no silent shadow state for the owner.

## Business models that fit dais

The business model has to be consistent with the product promise. If dais says
"own your social life," it cannot monetize by selling attention or inferred
private life.

### Managed hosting subscription

Model:

- self-hosted OSS remains free;
- paid dais.cloud hosting for people who want it to work like email hosting;
- tiers by storage, domains, media bandwidth, backups, support, and family/group
  seats.

Working examples:

- email/domain hosting: Fastmail, Proton, Google Workspace;
- managed open-source hosting: WordPress.com, Ghost(Pro), Discourse hosting;
- Mastodon paid hosting/support for organizations and independent managed
  instances.

Why it fits:

- Revenue grows with service quality, uptime, storage, and support, not
  addiction or surveillance.

### Premium personal features

Model:

- base server is functional;
- paid features add convenience: extra storage, media transcoding, custom
  domains, richer backups, advanced search, multi-account management, family
  admin, import/export automation.

Working examples:

- Discord Nitro sells customization, upload limits, profile features, and
  quality-of-life upgrades rather than requiring feed ads.
- Telegram Premium sells power-user features and limits while keeping the
  baseline messenger broadly accessible.

Fit caveat:

- Avoid features that make privacy or safety paid-only. Privacy and core safety
  are baseline.

### Creator/fan subscriptions

Model:

- optional public creator pages;
- paid posts/newsletters/media;
- optional subscriber-only ActivityPub/Bluesky-compatible public summaries;
- private subscriber groups when the creator explicitly chooses that posture.

Working examples:

- Substack takes a percentage of paid subscriptions and has reached millions of
  paid subscriptions.
- Patreon sells creator memberships and takes platform/payment fees.
- Ghost supports paid memberships for independent publishers.

Fit caveat:

- This should be an expansion, not the default wedge. Creator monetization can
  distort product incentives if it starts rewarding outrage, parasocial pressure,
  or growth-at-all-costs.

### Organization/community hosting

Model:

- paid managed servers for clubs, families, schools, professional associations,
  research groups, small businesses, and local communities;
- group actor, organization actor, moderation logs, member management, and
  archival/export tools.

Working examples:

- Discourse sells hosted community forums.
- Slack sells team/workspace collaboration.
- Discord monetizes communities through Nitro, server boosts, and emerging ads,
  showing that persistent group spaces have real value.

Fit caveat:

- Dais should not become general enterprise chat. The focus is social identity,
  posting, groups, and federation.

### Support, compliance, and concierge migration

Model:

- paid setup for custom domains and Cloudflare;
- import from Mastodon/Bluesky/RSS exports;
- backup/restore testing;
- family/community onboarding;
- compliance support for organizations.

Working examples:

- open-source companies commonly monetize support, hosting, and migration rather
  than product surveillance;
- WordPress, Discourse, Ghost, Mastodon hosting providers, and email hosts all
  validate this pattern.

Fit caveat:

- Keep migration/export tools available to self-hosters; do not create lock-in.

### Marketplace for user-controlled extensions

Model:

- paid optional extensions: theme packs, local AI moderation models, backup
  providers, media processors, custom importer connectors, analytics dashboards.

Working examples:

- WordPress themes/plugins;
- Shopify app ecosystem;
- Obsidian Sync/Publish plus plugin ecosystem.

Fit caveat:

- Extensions must not get privileged access to private graph/content without
  explicit owner approval and visible audit logs.

## Recommended product principles

1. **Relationship first.** Home is people and sources the owner chose.
2. **Private by default.** Public posting is a deliberate mode, not a mistake.
3. **Protocol honesty.** Never route private/friends content to public-only
   protocols.
4. **User agency over ranking.** Ranking is optional, inspectable, and locally
   controlled.
5. **Calm by design.** Stop states, quiet hours, and no engagement traps.
6. **Source context over virality.** Domain, author, original link, trust signal,
   and why-seen context matter more than raw counts.
7. **Moderation is explainable.** Local policy, transparent reasons, reversible
   actions, and owner review.
8. **Privacy is not premium.** Paid tiers may add capacity and convenience, not
   basic safety.
9. **Portability is a promise.** Export posts, media, graph, settings, blocks,
   groups, and keys where technically possible.
10. **Small groups are core.** Families, friends, clubs, and close communities
    are not edge cases; they are the product center.

## Platform-by-platform implementation implications

### Facebook

Build:

- friends/family Home;
- groups and events;
- memories/archives eventually;
- local community and organization actors.

Avoid:

- opaque News Feed ranking;
- graph/data monetization;
- confusing privacy settings;
- engagement-driven outrage and low-quality news.

### Instagram

Build:

- good media rendering;
- albums, short clips, link previews;
- DMs/replies with context;
- creator accounts as explicit public personas.

Avoid:

- like/follower count dominance;
- filters/beauty/status mechanics that intensify comparison;
- hidden ad/influencer incentives.

### Threads and X

Build:

- public text broadcasting;
- replies/quotes/reposts;
- lists/starter packs/custom feeds;
- journalist/news/source following.

Avoid:

- public-by-default personal posting;
- political/content amplification the owner did not request;
- paid visibility as a trust substitute.

### TikTok and YouTube

Build:

- explicit video/media watch mode;
- source subscriptions;
- save/share/reply flows;
- local "not interested" and topic controls that persist.

Avoid:

- autoplay/infinite recommendations in Home;
- algorithmic rabbit holes;
- unlabeled AI-generated/low-quality media;
- creator metrics that reward manipulative posting.

### Reddit

Build:

- topic/source watching;
- threaded discussion;
- question/answer community patterns;
- owner-selected public communities.

Avoid:

- platform capture of volunteer moderation labor;
- brittle API/tool access;
- karma as the main trust proxy.

### Discord

Build:

- private group spaces;
- roles/audience groups;
- channels for family/community contexts;
- integrations/bots only with visible permission scopes.

Avoid:

- closed network lock-in;
- sprawling notification defaults;
- private-server abuse invisibility.

### Telegram

Build:

- public/private channels as watchable sources;
- broadcast feeds;
- large media/file support;
- cross-device message reliability.

Avoid:

- weak abuse response;
- confusing E2EE claims;
- channel ads that the operator cannot control;
- crypto/financial complexity as a default product path.

### Discord and Telegram watch/import feasibility

Policy checked: 2026-06-26.

Recommendation:

- **Discord:** do not build a general Discord scraper, self-bot, user-token
  importer, or public index connector. Treat Discord as out of scope for direct
  watch ingestion until there is an owner-authorized bot integration for a
  server/channel the owner controls or administers.
- **Telegram:** support owner-authorized bot/channel imports only for channels
  or chats where the bot is deliberately added and allowed to receive updates.
  Do not scrape Telegram web views, broad channel directories, private chats, or
  user-account sessions.
- **Both:** allow manual import of owner-exported files or pasted URLs/text as
  local, owner-only source material, with provenance labels and no automatic
  reposting. These imports should not train models, populate public indexes, or
  bypass platform access controls.

Supported paths:

- Manual import: owner supplies an export, file, URL, or paste and Dais stores it
  as a local source with clear provenance and retention controls.
- Discord bot integration: only for a server/channel where the owner has
  permission to install the bot, with visible scopes and an explicit channel
  allowlist. Store only the messages needed for the stated reader feature.
- Telegram Bot API integration: only for channels/groups/chats where the bot is
  added by an authorized operator and receives updates through Telegram's Bot
  API. Channel-post and chat-member update access should be treated as explicit
  integration state, not public crawl permission.

Not appropriate:

- Discord API data mining/scraping, self-bots, user-token collection, private
  server mirroring, member profiling, or message-content use for model training.
  Discord's developer policy requires explicit user/server permission for
  actions, forbids credential/token collection, restricts API data use to the
  stated functionality, and prohibits mining or scraping Discord service data.
- Telegram broad scraping, indexing, harvesting, AI/ML dataset creation, private
  chat mirroring, or importing content from one context into another without
  explicit consent. Telegram's content licensing terms prohibit platform data
  scraping/indexing/harvesting outside ordinary intended platform use, with a
  limited exception for legitimate Telegram clients, bots, and mini apps.

Product decision:

- Direct connectors are **not** the default v1.32 path. Prioritize manual import
  and owner-authorized bots. Discord should remain a future bot-integration
  candidate, not a watch source. Telegram can be a future bot/channel connector
  if the owner explicitly configures the bot and Dais labels the source as
  Telegram, bot-mediated, and not end-to-end encrypted.

Sources:

- Discord Developer Policy:
  <https://support-dev.discord.com/hc/en-us/articles/8563934450327-Discord-Developer-Policy>
- Discord Developer Terms of Service:
  <https://support-dev.discord.com/hc/en-us/articles/8562894815383-Discord-Developer-Terms-of-Service>
- Telegram Bot API:
  <https://core.telegram.org/bots/api>
- Telegram Terms of Service:
  <https://telegram.org/tos>
- Telegram Content Licensing Terms:
  <https://telegram.org/tos/content-licensing>

### Snapchat

Build:

- lightweight close-friend media;
- direct and group messaging;
- temporary display options if clearly labeled.

Avoid:

- pretending disappearing content is privacy;
- risky friend suggestions for minors;
- weak sextortion/reporting flows.

### LinkedIn

Build:

- optional professional persona;
- organization actor;
- professional follows/sources.

Avoid:

- performative algorithmic engagement;
- mixing work identity into private family/friend posts.

## Research backlog

Future research should keep Discord and Telegram in scope alongside Facebook,
Instagram, Threads, TikTok, YouTube, X, Reddit, Snapchat, LinkedIn, WhatsApp,
Signal, Bluesky, Mastodon, and other ActivityPub servers.

Open questions:

- What exact set of group primitives should dais support first: family group,
  friend group, public channel, topic community, organization actor, or all of
  them behind one audience model?
- Should dais implement "stories" as private/friends media updates, or would
  that import too much Instagram/Snapchat pressure?
- How should Dais Desk display public discovery without making Home feel like a
  recommendation feed?
- What is the minimum safe E2EE DM/group feature set before marketing private
  groups to non-technical families?
- Should dais.cloud offer family plans and organization plans separately?
- What source/trust signals are useful without creating a centralized trust
  authority?
- What local AI moderation features can run cheaply on Cloudflare without
  sending private content to third-party model providers?
