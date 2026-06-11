# Dais Privacy Protection Guide

**Understanding privacy controls in dual-protocol social media**

---

## Overview

Dais bridges two social media protocols with different privacy models:
- **ActivityPub** - Supports multiple visibility levels
- **AT Protocol (Bluesky)** - All posts are public by default

This guide explains how Dais protects your privacy when posting across both networks.

---

## Privacy Levels Comparison

| Visibility | ActivityPub | AT Protocol (Bluesky) | Dais Behavior |
|------------|-------------|-----------------------|---------------|
| **Public** | Anyone can see | Anyone can see | ✅ Posts to both |
| **Unlisted** | Not in timelines | Not supported | ✅ Posts to both (public on Bluesky) |
| **Followers-only** | Only followers | Not supported | ⚠️ ActivityPub only |
| **Direct** | Private message | Not supported | ⚠️ ActivityPub only |

---

## Automatic Privacy Protection

### How It Works

When you attempt to post with `--protocol both` and a non-public visibility:

1. **Privacy Check** - Dais detects visibility is `followers` or `direct`
2. **Protocol Downgrade** - Automatically switches from `both` to `activitypub`
3. **Warning Message** - Shows clear explanation of what happened
4. **Database Update** - Updates post record to reflect actual delivery
5. **Safe Delivery** - Post only goes to ActivityPub, Bluesky blocked

### Example: Followers-Only Post

```bash
$ dais post create "Private thoughts for followers only" \
    --protocol both \
    --visibility followers

⚠️  Privacy Notice: Bluesky doesn't support 'followers' visibility
   All Bluesky posts are public. Posting to ActivityPub only to protect your privacy.

✓ Post created and delivered
   Protocol: activitypub (downgraded from: both)
```

**What happened:**
- Original request: Post to both networks with followers-only visibility
- Privacy protection: Detected Bluesky doesn't support followers-only
- Action taken: Blocked Bluesky delivery, posted to ActivityPub only
- Result: Your private post stays private, only your ActivityPub followers can see it

### Example: Direct Message

```bash
$ dais post create "Secret message" \
    --protocol both \
    --visibility direct

⚠️  Privacy Notice: Bluesky doesn't support 'direct' visibility
   All Bluesky posts are public. Posting to ActivityPub only to protect your privacy.

✓ Post created and delivered
   Protocol: activitypub (downgraded from: both)
```

**Result:** Direct message sent via ActivityPub only, not leaked to public Bluesky timeline.

---

## Privacy by Protocol

### ActivityPub (Mastodon, Pleroma, etc.)

**Supported Visibility Levels:**

1. **Public** (`--visibility public`)
   - Appears in public timelines
   - Federated to other servers
   - Visible to anyone
   - ✅ Safe for dual-protocol

2. **Unlisted** (`--visibility unlisted`)
   - Not in public timelines
   - Visible via direct link
   - Federated but not promoted
   - ✅ Safe for dual-protocol (treated as public on Bluesky)

3. **Followers-only** (`--visibility followers`)
   - Only your followers can see
   - Not federated publicly
   - Private to follower list
   - ⚠️ ActivityPub only (blocked from Bluesky)

4. **Direct** (`--visibility direct`)
   - Private message
   - Only mentioned users can see
   - End-to-end encrypted in transit
   - ⚠️ ActivityPub only (blocked from Bluesky)

### AT Protocol (Bluesky)

**Supported Visibility:**
- **Public only** - All posts are publicly visible
- No followers-only visibility
- No direct messages via posts
- Separate chat protocol: `chat.bsky.convo.*`

**Bluesky Chat:**
- Separate from public posts
- Private conversations (not posts)
- Not cross-protocol (Bluesky only)
- Access via TUI `x` shortcut

---

## Database Privacy

### Post Protocol Field

The `protocol` field in the database reflects **actual delivery**, not original intent:

**Before Privacy Protection:**
```sql
INSERT INTO posts (id, protocol, visibility, content)
VALUES (..., 'both', 'followers', '...');
```

**After Privacy Protection:**
```sql
UPDATE posts SET protocol = 'activitypub' WHERE id = '...';
```

**Result:** Database accurately shows post was delivered to ActivityPub only.

### Query Posts by Actual Protocol

```bash
# Find all dual-protocol posts
wrangler d1 execute DB --remote --command \
  "SELECT id, protocol FROM posts WHERE protocol = 'both'"

# Find privacy-protected posts
wrangler d1 execute DB --remote --command \
  "SELECT id, visibility FROM posts WHERE visibility IN ('followers', 'direct')"
```

---

## Privacy Best Practices

### 1. Understand Protocol Differences

- **ActivityPub** - Full privacy controls, federation-based
- **Bluesky** - Public by default, centralized discovery
- **Dais** - Bridges both, protects privacy automatically

### 2. Choose the Right Visibility

**Use `public` when:**
- You want maximum reach
- Content is appropriate for public timelines
- Cross-protocol posting desired

**Use `followers` when:**
- Sharing with trusted followers only
- Content is personal but not private
- ActivityPub-only delivery acceptable

**Use `direct` when:**
- Private conversations
- Sensitive information
- One-to-one or small group communication

### 3. Review Before Posting

**CLI Preview:**
```bash
dais post create "My post" --protocol both --visibility followers --dry-run
# Shows what will happen without actually posting
```

**TUI Composer:**
- Shows protocol selection before sending
- Visibility dropdown clearly labeled
- Warning if attempting cross-protocol private post

### 4. Enable Manual Follower Approval

```toml
# ~/.dais/config.toml
[server]
manually_approves_followers = true
```

**Benefits:**
- Review who can see your followers-only posts
- Block suspicious accounts before they see content
- Maintain control over follower list

### 5. Use Moderation Features

**Moderate replies to protect your space:**
```bash
# Open moderation queue
dais tui  # Press 'm'

# Review pending replies
# Approve legitimate replies
# Reject spam/harassment
```

**Block users/domains:**
```bash
# Block individual user
dais block user @spammer@evil.com

# Block entire domain
dais block domain evil.com
```

---

## Privacy Checklist

### Before Posting

- [ ] Check visibility setting matches intent
- [ ] Understand which protocol(s) will receive post
- [ ] Review content for sensitive information
- [ ] Consider if post should be cross-protocol

### After Posting

- [ ] Verify privacy warning if protocol downgraded
- [ ] Check database to confirm actual delivery
- [ ] Monitor replies for spam/harassment
- [ ] Review moderation queue regularly

### Ongoing

- [ ] Run `dais doctor` weekly to check health
- [ ] Review follower list monthly
- [ ] Update block list as needed
- [ ] Check Bluesky reply consumer logs

---

## Privacy vs. Functionality Tradeoff

### Maximum Privacy

```bash
# ActivityPub only, followers-only visibility
dais post create "Private post" --protocol activitypub --visibility followers
```

**Pros:**
- Full privacy controls
- Followers-only delivery
- No cross-protocol leaks

**Cons:**
- No Bluesky reach
- Smaller audience
- Not discoverable on Bluesky

### Maximum Reach

```bash
# Both protocols, public visibility
dais post create "Public announcement" --protocol both --visibility public
```

**Pros:**
- Maximum audience
- Cross-protocol discovery
- Full federation

**Cons:**
- Completely public
- No privacy controls on Bluesky
- Permanent public record

### Balanced Approach

**Public posts:** Use `both` for maximum reach
**Personal posts:** Use `activitypub` with `followers` visibility
**Private messages:** Use `activitypub` with `direct` visibility or Bluesky chat

---

## Privacy Violations & Reporting

### What Dais Prevents

- ✅ Leaking followers-only posts to public Bluesky
- ✅ Leaking direct messages to public timelines
- ✅ Cross-protocol privacy confusion
- ✅ Accidental public posting of private content

### What Dais Cannot Prevent

- ❌ Followers screenshotting posts
- ❌ Scrapers archiving public posts
- ❌ Federation to other servers (ActivityPub)
- ❌ Bluesky relay indexing (public posts only)

### Reporting Issues

**If you suspect a privacy leak:**
1. Document the issue (screenshots, logs)
2. File GitHub issue: [github.com/yourusername/dais/issues](https://github.com/yourusername/dais/issues)
3. Include: post ID, expected behavior, actual behavior
4. Mark as `security` label for priority review

---

## Advanced Privacy Features

### Custom Privacy Rules (Future)

```toml
# Future feature: Custom privacy rules
[privacy]
always_require_approval = true
auto_reject_new_domains = true
require_mutual_follow_for_dm = true
```

### Encrypted Posts (Future)

```bash
# Future feature: End-to-end encrypted posts
dais post create "Encrypted content" --encrypt --visibility followers
```

### Ephemeral Posts (Future)

```bash
# Future feature: Time-limited posts
dais post create "Temporary post" --expires-in 24h
```

---

## Privacy FAQ

**Q: Can I delete a post after it's federated?**
A: ActivityPub supports delete requests, but servers may have cached copies. Bluesky posts can be deleted via AT Protocol. Best practice: Don't post sensitive info publicly.

**Q: Are Bluesky chats private?**
A: Yes, `chat.bsky.convo.*` is separate from public posts and supports private conversations. Not cross-protocol with ActivityPub DMs.

**Q: Can I make my account private (protected)?**
A: ActivityPub supports `manuallyApprovesFollowers`. Enable in config. Bluesky doesn't support protected accounts currently.

**Q: What happens if I post something private by mistake?**
A: Delete immediately via TUI or CLI. Contact your instance admin if needed. Remember: federation may have propagated it already.

**Q: Are direct messages encrypted?**
A: ActivityPub DMs are encrypted in transit (HTTPS) but not end-to-end encrypted. Use Signal/Matrix for sensitive conversations.

---

## Summary

**Dais protects your privacy by:**
1. ✅ Automatic privacy checks before posting
2. ✅ Clear warnings when protocol downgraded
3. ✅ Database accuracy for actual delivery
4. ✅ Blocking cross-protocol privacy leaks
5. ✅ Manual follower approval
6. ✅ Comprehensive moderation tools

**Best practices:**
- Understand protocol differences
- Choose appropriate visibility
- Review before posting
- Enable manual approval
- Monitor moderation queue
- Block suspicious accounts

**Remember:** Once something is public, it's public forever. Use privacy controls wisely.

---

**For support:** See `USER_GUIDE.md` or run `dais doctor` for health checks.
