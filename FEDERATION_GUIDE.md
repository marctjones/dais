# Dais Federation Guide

**How to federate with Mastodon, Pleroma, and other ActivityPub servers**

---

## Table of Contents

1. [Introduction to Federation](#introduction-to-federation)
2. [How Federation Works](#how-federation-works)
3. [Federating with Your Dais Instance](#federating-with-your-dais-instance)
4. [Following Dais Users](#following-dais-users)
5. [Server-to-Server Communication](#server-to-server-communication)
6. [Troubleshooting Federation](#troubleshooting-federation)
7. [Instance Admin Guide](#instance-admin-guide)

---

## Introduction to Federation

**Federation** allows different social media servers to communicate with each other, creating a decentralized network.

### What is ActivityPub?

ActivityPub is the W3C standard protocol that powers:
- Mastodon
- Pleroma
- Misskey
- Pixelfed
- PeerTube
- **Dais**

### Key Concepts

**Actor** - A user or bot on the network
**Instance** - A server hosting actors (e.g., `mastodon.social`, `dais.social`)
**Federation** - Servers communicating with each other
**WebFinger** - Discovery mechanism for finding actors
**Inbox** - Where an actor receives activities
**Outbox** - Where an actor publishes activities

---

## How Federation Works

### Discovery Flow

```
User on Mastodon searches for: @social@dais.social
  ↓
1. Mastodon queries WebFinger: dais.social/.well-known/webfinger?resource=acct:social@dais.social
  ↓
2. Dais returns: {"subject": "acct:social@dais.social", "links": [...]}
  ↓
3. Mastodon follows link to Actor: https://social.dais.social/users/social
  ↓
4. Dais returns ActivityPub Actor JSON
  ↓
5. Mastodon displays profile and Follow button
```

### Follow Flow

```
Alice on Mastodon follows @social@dais.social
  ↓
1. Mastodon sends Follow activity to Dais inbox
  ↓
2. Dais stores follower request (status: pending)
  ↓
3. Dais actor reviews request via TUI (`dais tui` → press 'f')
  ↓
4. Actor approves → Dais sends Accept activity to Mastodon
  ↓
5. Mastodon updates: "You are now following @social@dais.social"
```

### Post Delivery Flow

```
@social@dais.social creates post via: dais post create "Hello!"
  ↓
1. Post stored in D1 database
  ↓
2. Delivery queue processes followers
  ↓
3. For each follower:
   - Sign HTTP request with actor's private key
   - POST Create activity to follower's inbox
  ↓
4. Follower's server receives post in inbox
  ↓
5. Post appears in follower's timeline
```

---

## Federating with Your Dais Instance

### For Mastodon Users

**Follow a Dais user:**

1. Search for `@username@your-dais-domain.com`
2. Click "Follow"
3. Wait for approval (if manual approval enabled)
4. Posts appear in your timeline

**Example:**
```
@social@dais.social
```

### For Pleroma Users

Same as Mastodon - ActivityPub is standardized:

1. Search for `@username@your-dais-domain.com`
2. Follow
3. Wait for approval
4. See posts in timeline

### For Other ActivityPub Instances

**Compatible with:**
- Mastodon
- Pleroma
- Akkoma
- Misskey
- GoToSocial
- Pixelfed (future - needs image support)
- PeerTube (future - needs video support)

**Standard ActivityPub implementation** - works with any compliant server.

---

## Following Dais Users

### Find Dais Users

**Option 1: Direct search**
```
Search: @username@dais-instance.com
```

**Option 2: WebFinger lookup**
```bash
curl "https://dais-instance.com/.well-known/webfinger?resource=acct:username@dais-instance.com"
```

**Option 3: Browse Outbox**
```
Visit: https://social.dais-instance.com/users/username/outbox
```

### Follow Request Process

**If manual approval is enabled** (default on Dais):

1. Send follow request
2. Dais actor receives notification
3. Actor reviews via TUI:
   ```bash
   dais tui
   # Press 'f' for Followers screen
   # Select pending request
   # Press 'a' to approve or 'x' to reject
   ```
4. You receive Accept/Reject activity
5. Follow confirmed or denied

**If auto-approval is enabled:**
- Follow request immediately accepted
- No manual review needed

---

## Server-to-Server Communication

### Endpoints

**Dais exposes standard ActivityPub endpoints:**

| Endpoint | Purpose | Example |
|----------|---------|---------|
| WebFinger | Discovery | `https://dais.social/.well-known/webfinger` |
| Actor | Profile | `https://social.dais.social/users/social` |
| Inbox | Receive activities | `https://social.dais.social/users/social/inbox` |
| Outbox | Published activities | `https://social.dais.social/users/social/outbox` |
| Followers | Follower list | `https://social.dais.social/users/social/followers` |
| Following | Following list | `https://social.dais.social/users/social/following` |

### HTTP Signatures

**All server-to-server requests are signed** using HTTP Signatures (RFC draft).

**Verification:**
1. Remote server sends signed POST to Dais inbox
2. Dais extracts signature from `Signature` header
3. Dais fetches remote actor's public key
4. Dais verifies signature matches request body
5. If valid → process activity
6. If invalid → reject with 401 Unauthorized

**Example signature header:**
```
Signature: keyId="https://mastodon.social/users/alice#main-key",
  headers="(request-target) host date digest",
  signature="Base64EncodedSignature..."
```

### Activity Types

**Dais handles these ActivityPub activities:**

**Inbound (received in inbox):**
- `Follow` - Follow request
- `Accept` - Follow acceptance
- `Reject` - Follow rejection
- `Create` - New post/reply
- `Update` - Edit post/profile
- `Delete` - Delete post
- `Like` - Like post (future)
- `Announce` - Boost/repost (future)
- `Undo` - Undo previous activity

**Outbound (sent from outbox):**
- `Create` - New post
- `Accept` - Accept follow request
- `Reject` - Reject follow request
- `Delete` - Delete post
- `Update` - Edit post (future)

---

## Troubleshooting Federation

### Common Issues

**Issue: "User not found" when searching**

**Possible causes:**
1. WebFinger endpoint not responding
2. DNS not configured correctly
3. SSL certificate issues
4. Firewall blocking requests

**Solution:**
```bash
# Test WebFinger locally
curl "https://your-domain.com/.well-known/webfinger?resource=acct:username@your-domain.com"

# Expected: JSON response with actor link
# If 404: Check DNS and worker deployment
```

**Issue: "Follow request stuck pending"**

**Possible causes:**
1. Inbox worker not receiving requests
2. Database not storing follower
3. Manual approval enabled but actor not reviewing

**Solution:**
```bash
# Check inbox is responding
curl -X POST https://social.your-domain.com/users/username/inbox \
  -H "Content-Type: application/activity+json"

# Check database for pending followers
wrangler d1 execute DB --remote --command \
  "SELECT * FROM followers WHERE status = 'pending'"

# Approve via CLI
dais followers approve @user@remote-instance.com
```

**Issue: "Posts not appearing in remote timelines"**

**Possible causes:**
1. Outbox worker not delivering
2. HTTP signature verification failing
3. Remote server blocking your instance
4. Delivery queue not processing

**Solution:**
```bash
# Check outbox is accessible
curl https://social.your-domain.com/users/username/outbox \
  -H "Accept: application/activity+json"

# Check delivery logs (if monitoring enabled)
wrangler tail actor --env production

# Test delivery manually
dais post create "Test federation" --protocol activitypub
```

**Issue: "403 Forbidden from remote server"**

**Possible causes:**
1. HTTP signature invalid
2. Clock skew (time difference > 30 seconds)
3. Remote server requires specific headers
4. Your instance is blocked

**Solution:**
```bash
# Check server time
date -u

# Ensure NTP is synchronized
# Remote server may be checking Date header

# Test with curl (manual signature)
# See: https://docs.joinmastodon.org/spec/security/#http
```

---

## Instance Admin Guide

### Setting Up Federation

**Prerequisites:**
1. Domain name with DNS configured
2. SSL certificate (Cloudflare provides automatically)
3. Dais deployed to Cloudflare Workers
4. All health checks passing

**DNS Configuration:**

```
# Required DNS records
dais.social               → Cloudflare Workers (landing)
social.dais.social        → Cloudflare Workers (actor/inbox/outbox)
pds.dais.social           → Cloudflare Workers (AT Protocol PDS)

# Optional
*.dais.social             → Cloudflare Workers (catch-all)
```

**Verify Setup:**
```bash
# Run health check
dais doctor

# Expected: ✓ All checks passed (9/9)
```

### Configuring Follower Approval

**Enable manual approval** (recommended for privacy):

Edit `~/.dais/config.toml`:
```toml
[server]
manually_approves_followers = true
```

**Or disable for auto-accept:**
```toml
[server]
manually_approves_followers = false
```

**Apply changes:**
```bash
# Redeploy workers to pick up new config
dais deploy workers
```

### Blocking Instances

**Block individual user:**
```bash
dais block user @spammer@evil.com
```

**Block entire instance:**
```bash
dais block domain evil.com
```

**Blocked instances:**
- Cannot follow you
- Cannot send you posts/replies
- Cannot appear in your timeline
- Existing follows removed

**Unblock:**
```bash
dais block unblock @user@instance.com
dais block unblock-domain instance.com
```

### Federation Privacy

**Public posts:**
- Federated to all followers
- Appear in public timelines
- Discoverable via search
- Cached by remote servers

**Followers-only posts:**
- Only sent to followers
- Not in public timelines
- Still cached by follower servers
- Not truly private (followers can screenshot)

**Direct messages:**
- Only sent to mentioned users
- Not federated publicly
- Encrypted in transit (HTTPS)
- Not end-to-end encrypted

### Rate Limiting (Future)

**Current:** No rate limiting implemented
**Future:** Cloudflare Workers rate limiting per remote IP

```toml
# Future config
[federation]
max_requests_per_minute = 60
max_follows_per_hour = 10
```

### Monitoring Federation

**Check follower count:**
```bash
dais followers list | wc -l
```

**View recent federation activity:**
```bash
wrangler d1 execute DB --remote --command \
  "SELECT actor_id, activity_type, created_at FROM activities ORDER BY created_at DESC LIMIT 20"
```

**Watch live requests:**
```bash
wrangler tail inbox --env production
```

---

## Protocol Specifications

### ActivityPub

**Official Spec:** https://www.w3.org/TR/activitypub/

**Key sections:**
- Actor objects
- Activity delivery
- Collections (inbox, outbox, followers, following)
- Server-to-server interactions

### WebFinger

**RFC:** https://www.rfc-editor.org/rfc/rfc7033

**Dais implementation:**
```json
{
  "subject": "acct:social@dais.social",
  "aliases": [
    "https://social.dais.social/users/social"
  ],
  "links": [
    {
      "rel": "self",
      "type": "application/activity+json",
      "href": "https://social.dais.social/users/social"
    }
  ]
}
```

### HTTP Signatures

**Draft Spec:** https://datatracker.ietf.org/doc/html/draft-cavage-http-signatures

**Dais implementation:**
- Signs all outbound POST requests
- Verifies all inbound POST requests
- Uses RSA-SHA256 algorithm
- Includes headers: `(request-target)`, `host`, `date`, `digest`

---

## Best Practices

### For Dais Admins

1. **Enable manual follower approval** - Review who follows you
2. **Block spam instances** - Maintain block list
3. **Monitor federation logs** - Watch for abuse
4. **Keep software updated** - Security fixes
5. **Backup database regularly** - Follower data is valuable

### For Remote Instance Admins

1. **Respect manual approval** - Don't retry rejected follows
2. **Implement rate limiting** - Prevent spam
3. **Verify HTTP signatures** - Security best practice
4. **Cache actor profiles** - Reduce lookups
5. **Honor Delete activities** - Respect post deletion

---

## FAQ

**Q: Can Dais federate with Mastodon?**
A: Yes! Dais implements standard ActivityPub and federates with all compatible servers.

**Q: Do I need to configure anything special to federate?**
A: No. If `dais doctor` shows all checks passing, federation works automatically.

**Q: Can I federate with Bluesky?**
A: Partially. Dais can POST to Bluesky via AT Protocol, but Bluesky doesn't federate back to ActivityPub. The consumer captures Bluesky replies for unified thread viewing.

**Q: Are my posts cached by remote servers?**
A: Yes. Public posts are cached indefinitely by follower servers. Deletion sends a Delete activity, but caching is up to remote servers.

**Q: Can I restrict who can follow me?**
A: Yes. Enable `manually_approves_followers = true` in config, then approve/reject via TUI.

**Q: What happens if I block an instance?**
A: All activities from that instance are rejected. Existing followers are removed. No federation with blocked instances.

**Q: Can I see who follows me?**
A: Yes. Run `dais followers list` or press `f` in TUI.

**Q: How do I unfederate from an instance?**
A: Block the domain: `dais block domain instance.com`. All communication stops.

---

## Support

**Issues:** https://github.com/yourusername/dais/issues
**ActivityPub Spec:** https://www.w3.org/TR/activitypub/
**Mastodon Docs:** https://docs.joinmastodon.org/

---

**Happy federating! 🌐**
