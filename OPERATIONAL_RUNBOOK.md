# Dais Operational Runbook

**Incident response, troubleshooting, and day-to-day operations guide**

---

## Table of Contents

1. [Daily Operations](#daily-operations)
2. [Health Monitoring](#health-monitoring)
3. [Incident Response](#incident-response)
4. [Common Issues & Solutions](#common-issues--solutions)
5. [Emergency Procedures](#emergency-procedures)
6. [Maintenance Tasks](#maintenance-tasks)
7. [Performance Optimization](#performance-optimization)

---

## Daily Operations

### Morning Health Check (5 minutes)

```bash
# 1. Run diagnostics
dais doctor

# Expected: ✓ All checks passed (9/9)
# If failures: See "Common Issues" section

# 2. Check Bluesky consumer
ps aux | grep bluesky_reply_consumer

# Expected: One running process
# If not running: tmux attach -t bluesky-consumer to check logs

# 3. Check recent activity
dais followers list | head -10
dais post list --limit 5 (future command)

# 4. Review moderation queue
dais tui
# Press 'm' → Check pending count
# If pending > 10: Review and moderate
```

### Weekly Tasks (15 minutes)

```bash
# Monday: Review and clean up
1. Review follower requests (approve/reject)
2. Check moderation queue (clear spam)
3. Review block list (add new domains if needed)
4. Check disk space usage

# Wednesday: Backup verification
1. Verify latest backup exists
2. Check backup file size (should be consistent)
3. Test restore to temporary directory

# Friday: Performance check
1. Check D1 database size
2. Review consumer logs for errors
3. Check worker error rates (if monitoring enabled)
```

### Monthly Tasks (1 hour)

```bash
# First of month:
1. Full backup to cold storage
2. Review follower growth trends
3. Update block list from community sources
4. Check for dais updates: git pull

# Test restore procedure:
1. Create test environment
2. Restore from backup
3. Verify data integrity
4. Document any issues
```

---

## Health Monitoring

### Quick Health Check

```bash
#!/bin/bash
# scripts/health-check.sh

echo "=== Dais Health Check ==="
echo

# 1. Diagnostics
echo "[1/6] Running dais doctor..."
dais doctor || echo "⚠ Health check failed"

# 2. Consumer status
echo
echo "[2/6] Checking Bluesky consumer..."
if ps aux | grep -q "[b]luesky_reply_consumer"; then
    echo "  ✓ Consumer running"
else
    echo "  ✗ Consumer NOT running"
fi

# 3. Database connectivity
echo
echo "[3/6] Testing database..."
if wrangler d1 execute DB --remote --command "SELECT 1" &>/dev/null; then
    echo "  ✓ Database accessible"
else
    echo "  ✗ Database NOT accessible"
fi

# 4. Worker endpoints
echo
echo "[4/6] Testing WebFinger..."
if curl -sf "https://dais.social/.well-known/webfinger?resource=acct:social@dais.social" > /dev/null; then
    echo "  ✓ WebFinger responding"
else
    echo "  ✗ WebFinger NOT responding"
fi

# 5. Disk space
echo
echo "[5/6] Checking disk space..."
USAGE=$(df -h ~/.dais | awk 'NR==2 {print $5}' | tr -d '%')
if [ $USAGE -lt 80 ]; then
    echo "  ✓ Disk usage: ${USAGE}%"
else
    echo "  ⚠ Disk usage high: ${USAGE}%"
fi

# 6. Backup freshness
echo
echo "[6/6] Checking backup freshness..."
LATEST_BACKUP=$(find ~/.dais/backups -name "dais_backup_*.tar.gz*" -type f -mtime -2 | head -1)
if [ -n "$LATEST_BACKUP" ]; then
    echo "  ✓ Backup within last 48 hours"
    echo "    $(basename $LATEST_BACKUP)"
else
    echo "  ⚠ No recent backup found"
fi

echo
echo "=== Health Check Complete ==="
```

### Monitoring Metrics

**Key Performance Indicators (KPIs):**

| Metric | Healthy Range | Warning | Critical |
|--------|---------------|---------|----------|
| Database Size | < 500 MB | 500-900 MB | > 900 MB |
| Follower Count Growth | +1-10/day | +50/day | +100/day (spam) |
| Pending Moderation | 0-5 | 6-20 | > 20 |
| Consumer Uptime | 99%+ | 95-99% | < 95% |
| Worker Error Rate | < 1% | 1-5% | > 5% |

---

## Incident Response

### Incident Severity Levels

**P0 - Critical (Immediate Response)**
- Complete service outage
- Data loss
- Security breach
- Private key compromised

**P1 - High (Response within 1 hour)**
- Federation broken
- Database inaccessible
- Consumer down > 30 minutes
- Cannot create posts

**P2 - Medium (Response within 4 hours)**
- Slow performance
- Consumer errors (but running)
- Moderation queue backlog
- Single worker down

**P3 - Low (Response within 24 hours)**
- UI bugs
- Documentation errors
- Feature requests

### P0 - Critical Incident Response

**Service Completely Down:**

```bash
# Step 1: Assess
echo "Incident detected at $(date)" >> ~/incident.log
dais doctor 2>&1 | tee -a ~/incident.log

# Step 2: Check DNS
dig dais.social
dig social.dais.social

# Step 3: Check Cloudflare status
# Visit: https://www.cloudflarestatus.com/

# Step 4: Check workers
wrangler deployments list --name=actor --env=production

# Step 5: Emergency redeploy
dais deploy workers

# Step 6: Verify restoration
dais test webfinger
dais test actor

# Step 7: Document incident
echo "Resolution: ..." >> ~/incident.log
```

**Data Loss Detected:**

```bash
# Step 1: STOP ALL WRITES immediately
# - Stop consumer: pkill -f bluesky_reply_consumer
# - Pause post creation
# - Document what data is missing

# Step 2: Identify backup to restore
ls -lt ~/.dais/backups/

# Step 3: Follow BACKUP_RESTORE.md procedures

# Step 4: Verify restoration
wrangler d1 execute DB --remote --command \
  "SELECT COUNT(*) FROM posts"

# Step 5: Resume operations
```

**Security Breach:**

```bash
# Step 1: IMMEDIATE ACTIONS
# - Rotate all credentials
# - Block suspicious IPs
# - Review access logs

# Step 2: Regenerate keys
dais setup init --regenerate-keys

# Step 3: Redeploy with new keys
dais deploy secrets
dais deploy workers

# Step 4: Notify users (if data exposed)
# Step 5: Review and patch vulnerability
# Step 6: Incident report
```

---

## Common Issues & Solutions

### Issue 1: "WebFinger Endpoint Not Responding"

**Symptoms:**
- `dais test webfinger` fails
- Remote users cannot find your account
- HTTP 404 or 502 errors

**Diagnosis:**
```bash
# Test directly
curl -v "https://dais.social/.well-known/webfinger?resource=acct:social@dais.social"

# Check DNS
dig dais.social

# Check worker deployment
wrangler deployments list --name=webfinger --env=production
```

**Solutions:**

**Solution A: Redeploy worker**
```bash
dais deploy workers
# Wait 30 seconds
dais test webfinger
```

**Solution B: Check DNS configuration**
```bash
# Verify DNS points to Cloudflare Workers
# Should see Cloudflare IPs (e.g., 104.21.x.x)
dig +short dais.social
```

**Solution C: Check wrangler.toml**
```bash
# Verify routes are correct
cat workers/webfinger/wrangler.toml

# Should include:
# [[env.production.routes]]
# pattern = "dais.social/.well-known/webfinger*"
```

---

### Issue 2: "Bluesky Consumer Not Capturing Replies"

**Symptoms:**
- Consumer running but no replies stored
- Stats show commits processed but 0 replies
- Bluesky replies not appearing in TUI

**Diagnosis:**
```bash
# Check consumer logs
tmux attach -t bluesky-consumer

# Look for:
# - Connection errors
# - Parse errors
# - Database write errors

# Check database for posts with AT Protocol URIs
wrangler d1 execute DB --remote --command \
  "SELECT id, atproto_uri FROM posts WHERE atproto_uri IS NOT NULL"
```

**Solutions:**

**Solution A: No posts with AT URIs**
```bash
# Create test dual-protocol post
dais post create "Test for Bluesky replies" --protocol both --remote

# Verify AT URI was saved
wrangler d1 execute DB --remote --command \
  "SELECT atproto_uri FROM posts ORDER BY published_at DESC LIMIT 1"
```

**Solution B: Consumer connection issues**
```bash
# Restart consumer
tmux kill-session -t bluesky-consumer
tmux new-session -d -s bluesky-consumer \
  "cd services && python bluesky_reply_consumer.py --remote"

# Check for connection
sleep 5
tmux capture-pane -t bluesky-consumer -p | tail -20
```

**Solution C: Database permissions**
```bash
# Test database write
wrangler d1 execute DB --remote --command \
  "INSERT INTO replies (id, post_id, actor_id, actor_username, content, published_at) \
   VALUES ('test123', 'post123', 'actor123', '@test', 'test content', '2026-01-01T00:00:00Z')"

# Clean up test
wrangler d1 execute DB --remote --command \
  "DELETE FROM replies WHERE id = 'test123'"
```

---

### Issue 3: "Federation Broken - No Posts Reaching Followers"

**Symptoms:**
- Posts created successfully
- Followers exist in database
- But posts not appearing in follower timelines

**Diagnosis:**
```bash
# Check followers
wrangler d1 execute DB --remote --command \
  "SELECT actor_id, inbox_url FROM followers WHERE status='accepted' LIMIT 5"

# Check outbox
curl -H "Accept: application/activity+json" \
  https://social.dais.social/users/social/outbox | jq '.orderedItems | length'

# Test delivery manually (requires signing)
# Check worker logs
wrangler tail outbox --env=production
```

**Solutions:**

**Solution A: HTTP Signature issues**
```bash
# Verify private key exists
ls -la ~/.dais/keys/private.pem

# Redeploy secrets
dais deploy secrets

# Test with single follower
# (Manual HTTP signature test - see FEDERATION_GUIDE.md)
```

**Solution B: Delivery queue not processing**
```bash
# Check delivery queue worker
wrangler deployments list --name=delivery-queue --env=production

# Redeploy delivery queue
cd workers/delivery-queue
wrangler deploy --env=production
```

**Solution C: Remote server blocking**
```bash
# Check if remote server is blocking your instance
# Contact remote admin
# Check their instance blocklist

# Test with different instance
dais followers list | grep -v "blocked-instance.com"
```

---

### Issue 4: "Database Full - D1 Size Limit Reached"

**Symptoms:**
- Cannot create new posts
- INSERT operations failing
- Error: "database or disk is full"

**Diagnosis:**
```bash
# Check database size
wrangler d1 info DB --remote

# Check row counts
wrangler d1 execute DB --remote --command \
  "SELECT
     (SELECT COUNT(*) FROM posts) as posts,
     (SELECT COUNT(*) FROM replies) as replies,
     (SELECT COUNT(*) FROM followers) as followers"
```

**Solutions:**

**Solution A: Archive old data**
```bash
# Export old posts (>1 year)
wrangler d1 execute DB --remote --command \
  "SELECT * FROM posts WHERE published_at < date('now', '-1 year')" \
  --json > archive_old_posts.json

# Delete archived posts
wrangler d1 execute DB --remote --command \
  "DELETE FROM posts WHERE published_at < date('now', '-1 year')"

# Vacuum database
wrangler d1 execute DB --remote --command "VACUUM"
```

**Solution B: Cleanup spam/rejected content**
```bash
# Delete rejected replies
wrangler d1 execute DB --remote --command \
  "DELETE FROM replies WHERE moderation_status = 'rejected' \
   AND created_at < date('now', '-30 days')"

# Delete old notifications
wrangler d1 execute DB --remote --command \
  "DELETE FROM notifications WHERE created_at < date('now', '-90 days')"
```

**Solution C: Migrate to larger database**
```bash
# Create new D1 database (if limit reached)
wrangler d1 create dais-db-v2 --remote

# Export from old database
wrangler d1 export DB --remote --output=full_export.sql

# Import to new database
wrangler d1 execute dais-db-v2 --remote --file=full_export.sql

# Update config with new database ID
# Redeploy workers
dais deploy workers
```

---

### Issue 5: "TUI Crashes on Startup"

**Symptoms:**
- `dais tui` command crashes
- Python traceback shown
- Cannot access TUI interface

**Diagnosis:**
```bash
# Run with verbose error output
python -c "from cli.dais_cli.tui.app import DaisApp; DaisApp().run()"

# Check Python version
python --version  # Should be 3.11+

# Check dependencies
pip list | grep textual
```

**Solutions:**

**Solution A: Reinstall dependencies**
```bash
cd cli
pip install -e . --force-reinstall
```

**Solution B: Clear Textual cache**
```bash
rm -rf ~/.cache/textual/
```

**Solution C: Database connection issue**
```bash
# Test database access
dais doctor

# If database accessible, check specific table
wrangler d1 execute DB --remote --command "SELECT 1 FROM posts LIMIT 1"
```

---

## Emergency Procedures

### Emergency Shutdown

**When to use:** Suspected attack, abuse, or security incident

```bash
#!/bin/bash
# scripts/emergency-shutdown.sh

echo "=== EMERGENCY SHUTDOWN ==="
echo "Initiated at $(date)"
echo

# 1. Stop consumer
echo "[1/3] Stopping Bluesky consumer..."
pkill -f bluesky_reply_consumer
tmux kill-session -t bluesky-consumer 2>/dev/null
echo "  ✓ Consumer stopped"

# 2. Disable workers (set to maintenance mode)
echo "[2/3] Disabling workers..."
# NOTE: Requires custom maintenance worker
# For now, document that manual intervention needed at Cloudflare dashboard

# 3. Backup current state
echo "[3/3] Creating emergency backup..."
./scripts/backup.sh

echo
echo "✓ Emergency shutdown complete"
echo "To restore: See OPERATIONAL_RUNBOOK.md 'Emergency Restore'"
```

### Emergency Restore

```bash
#!/bin/bash
# scripts/emergency-restore.sh

echo "=== EMERGENCY RESTORE ==="
echo "Started at $(date)"
echo

# 1. Restore from latest backup
LATEST_BACKUP=$(find ~/.dais/backups -name "dais_backup_*.tar.gz*" -type f | sort -r | head -1)
echo "[1/4] Restoring from: $LATEST_BACKUP"
# Follow BACKUP_RESTORE.md procedures

# 2. Redeploy all workers
echo "[2/4] Redeploying workers..."
dais deploy workers

# 3. Restart consumer
echo "[3/4] Starting consumer..."
tmux new-session -d -s bluesky-consumer \
  "cd services && python bluesky_reply_consumer.py --remote"

# 4. Verify health
echo "[4/4] Verifying health..."
sleep 10
dais doctor

echo
echo "✓ Emergency restore complete"
```

---

## Maintenance Tasks

### Monthly Database Maintenance

```bash
#!/bin/bash
# scripts/monthly-maintenance.sh

echo "=== Monthly Maintenance - $(date +%Y-%m) ==="

# 1. Vacuum database
echo "[1/5] Vacuuming database..."
wrangler d1 execute DB --remote --command "VACUUM"

# 2. Update statistics
echo "[2/5] Updating statistics..."
wrangler d1 execute DB --remote --command "ANALYZE"

# 3. Archive old content
echo "[3/5] Archiving old rejected replies..."
wrangler d1 execute DB --remote --command \
  "DELETE FROM replies
   WHERE moderation_status = 'rejected'
   AND created_at < date('now', '-90 days')"

# 4. Check for orphaned data
echo "[4/5] Checking for orphaned replies..."
ORPHANED=$(wrangler d1 execute DB --remote --command \
  "SELECT COUNT(*) FROM replies r
   WHERE NOT EXISTS (SELECT 1 FROM posts p WHERE p.id = r.post_id)" \
   --json | jq '.[0].results[0]["COUNT(*)"]')
echo "  Found $ORPHANED orphaned replies"

# 5. Full backup
echo "[5/5] Creating monthly backup..."
./scripts/backup.sh

echo
echo "✓ Monthly maintenance complete"
```

### Weekly Performance Tuning

```bash
# Check slow queries (future - requires logging)
# Optimize indexes
# Review and clean up blocks
# Update blocklists from community sources
```

---

## Performance Optimization

### Database Performance

**Add indexes for common queries:**

```sql
-- Index for replies by post_id (already exists)
CREATE INDEX IF NOT EXISTS idx_replies_post_id ON replies(post_id);

-- Index for followers by status
CREATE INDEX IF NOT EXISTS idx_followers_status ON followers(status);

-- Index for posts by published_at
CREATE INDEX IF NOT EXISTS idx_posts_published ON posts(published_at DESC);

-- Index for notifications
CREATE INDEX IF NOT EXISTS idx_notifications_created ON notifications(created_at DESC);
```

**Query optimization:**

```sql
-- Bad: SELECT * FROM posts ORDER BY published_at DESC LIMIT 20
-- Good: SELECT id, content, published_at FROM posts ORDER BY published_at DESC LIMIT 20
-- (Only select needed columns)
```

### Worker Performance

**Reduce cold starts:**
- Keep workers warm with periodic requests
- Use Cloudflare's Smart Placement

**Optimize database queries:**
- Cache frequently accessed data
- Use prepared statements
- Limit result sets

### Consumer Performance

**Optimize firehose processing:**

```python
# Current: Process every commit
# Optimization: Filter early

def should_process_commit(commit):
    """Skip commits that can't be replies."""
    # Skip if no operations
    if not commit.ops:
        return False

    # Skip if no post creates
    if not any(op.action == 'create' and 'post' in op.path for op in commit.ops):
        return False

    return True

# Use in consumer
if should_process_commit(commit):
    self.handle_commit(commit)
```

---

## Alerts & Notifications

### Email Alerts (Future)

```bash
# Setup email on critical events
# - Backup failures
# - Consumer crashes
# - Database full
# - Security incidents

# Example with sendmail
echo "Consumer crashed at $(date)" | \
  mail -s "ALERT: Dais Consumer Down" admin@example.com
```

### Slack/Discord Webhooks (Future)

```bash
# Send webhook on incidents
curl -X POST https://hooks.slack.com/services/YOUR/WEBHOOK/URL \
  -H 'Content-Type: application/json' \
  -d '{"text":"Dais: Consumer crashed"}'
```

---

## Runbook Checklist

### New Incident

- [ ] Classify severity (P0-P3)
- [ ] Document start time
- [ ] Follow appropriate incident response
- [ ] Monitor for resolution
- [ ] Verify fix
- [ ] Document in incident log
- [ ] Post-mortem if P0/P1

### Daily Checklist

- [ ] Run `dais doctor`
- [ ] Check consumer status
- [ ] Review moderation queue
- [ ] Check for urgent follower requests

### Weekly Checklist

- [ ] Review and approve followers
- [ ] Clean moderation queue
- [ ] Verify backups
- [ ] Check performance metrics
- [ ] Review block list

### Monthly Checklist

- [ ] Full backup to cold storage
- [ ] Database maintenance
- [ ] Test restore procedure
- [ ] Review documentation
- [ ] Check for updates

---

## Support Resources

**Documentation:**
- `USER_GUIDE.md` - User features
- `FEDERATION_GUIDE.md` - Federation troubleshooting
- `BACKUP_RESTORE.md` - Restore procedures
- `API_DOCUMENTATION.md` - Developer reference

**Commands:**
- `dais doctor` - Health diagnostics
- `dais --help` - CLI help
- `scripts/health-check.sh` - Quick health check
- `scripts/backup.sh` - Backup script

**External Resources:**
- Cloudflare Status: https://www.cloudflarestatus.com/
- ActivityPub Spec: https://www.w3.org/TR/activitypub/
- GitHub Issues: https://github.com/yourusername/dais/issues

---

**Stay calm, follow the runbook, and you'll be fine! 🧯**
