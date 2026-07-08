# Dais Backup & Restore Procedures

**Comprehensive guide for backing up and restoring your Dais instance**

---

## Table of Contents

1. [What to Backup](#what-to-backup)
2. [Automated Backup Scripts](#automated-backup-scripts)
3. [Manual Backup Procedures](#manual-backup-procedures)
4. [Restore Procedures](#restore-procedures)
5. [Disaster Recovery](#disaster-recovery)
6. [Backup Schedule Recommendations](#backup-schedule-recommendations)

---

## What to Backup

### Critical Data

**Must backup (irreplaceable):**
1. **D1 Database** - Posts, followers, replies, messages
2. **Private Key** - RSA private key for HTTP signatures
3. **PDS Password** - AT Protocol authentication
4. **Configuration** - Server settings and Cloudflare credentials

**Important but replaceable:**
5. **R2 Media** - Images, videos (if implemented)
6. **Worker Code** - Stored in git repository

### Data Locations

| Data | Location | Backup Priority |
|------|----------|-----------------|
| Database | Cloudflare D1 | **CRITICAL** |
| Private Key | `~/.dais/keys/private.pem` | **CRITICAL** |
| Public Key | `~/.dais/keys/public.pem` | Important |
| PDS Password | `~/.dais/pds-password.txt` | **CRITICAL** |
| Config | `~/.dais/config.toml` | **CRITICAL** |
| Media Files | Cloudflare R2 | Important |
| Worker Code | Git repository | Low (version controlled) |

---

## Automated Backup Scripts

### Daily Backup Script

**File:** `scripts/backup.sh`

`scripts/backup.sh` is the current backup entry point. It uses the active router
worker configuration, writes a `dais-backup-v1` manifest, exports D1 SQL,
records Cloudflare backup/R2 inventory metadata when available, includes local
owner/key material when present, and verifies the final archive with
`scripts/verify-backup-archive.sh`.

**Usage:**
```bash
DAIS_BACKUP_PASSPHRASE='use-a-real-secret' scripts/backup.sh --env production
DAIS_BACKUP_PASSPHRASE='use-a-real-secret' scripts/backup.sh --env skpt
scripts/backup.sh --skip-cloud --no-encrypt --output-dir tmp/backup-test
scripts/verify-backup-archive.sh ~/.dais/backups/dais_production_backup_YYYYMMDDTHHMMSSZ.tar.gz.gpg
scripts/verify-backup-restore.sh ~/.dais/backups/dais_production_backup_YYYYMMDDTHHMMSSZ.tar.gz.gpg
```

Use `--no-encrypt` only for local tests with non-secret fixture data. Production
backups contain owner tokens, private keys, and recovery material when available.

### Restore Verification Harness

`scripts/verify-backup-restore.sh` is the restore gate for managed operations.
It extracts the archive, imports `database.sql` into a fresh local SQLite
database, and verifies that the restored schema includes the Dais portability
families: actors/profile data, posts, media metadata, follower/following graph,
settings, moderation settings, blocks, audiences, source/watch subscriptions and
items, local/peer E2EE devices, and encrypted messages.

Run the built-in harness test:

```bash
scripts/verify-backup-restore.sh --self-test
```

Run it against a real backup archive:

```bash
DAIS_BACKUP_PASSPHRASE_FILE=~/.dais/backup-passphrase \
  scripts/verify-backup-restore.sh ~/.dais/backups/dais_production_backup_YYYYMMDDTHHMMSSZ.tar.gz.gpg
```

Backups created with `--skip-cloud` intentionally contain placeholder SQL. The
restore verifier rejects those by default; pass `--allow-placeholder-sql` only
when you are checking archive packaging rather than restore coverage.

### Automated Backup with Cron

**Setup daily backup at 2 AM:**

```bash
# Edit crontab
crontab -e

# Add this line:
0 2 * * * cd /home/user/Projects/dais && DAIS_BACKUP_PASSPHRASE_FILE=/home/user/.dais/backup-passphrase scripts/backup.sh --env production >> /home/user/.dais/backups/backup.log 2>&1
```

---

## Manual Backup Procedures

### 1. Backup D1 Database

**Method A: Using wrangler CLI**

```bash
# Create backup
cd platforms/cloudflare/workers/router
wrangler d1 backup create DB --remote --env production

# List backups
wrangler d1 backup list DB --remote --env production

# Download specific backup
wrangler d1 backup download DB <backup-id> --remote --env production --output database.sql
```

**Method B: Export to SQL**

```bash
# Export all data
wrangler d1 export DB --remote --output=database_export.sql

# Export specific table
wrangler d1 execute DB --remote --command \
  "SELECT * FROM posts" --json > posts_backup.json
```

### 2. Backup Keys

```bash
# Backup directory
mkdir -p ~/dais-backup/keys

# Copy keys
cp ~/.dais/keys/private.pem ~/dais-backup/keys/
cp ~/.dais/keys/public.pem ~/dais-backup/keys/

# Verify
ls -la ~/dais-backup/keys/
```

**Security Note:** Private key is **extremely sensitive**. Encrypt before storing:

```bash
# Encrypt private key
gpg --symmetric --cipher-algo AES256 \
    --output ~/dais-backup/keys/private.pem.gpg \
    ~/.dais/keys/private.pem

# Delete unencrypted copy
rm ~/dais-backup/keys/private.pem
```

### 3. Backup PDS Password

```bash
# Backup password file
cp ~/.dais/pds-password.txt ~/dais-backup/

# Or encrypt
gpg --symmetric --cipher-algo AES256 \
    --output ~/dais-backup/pds-password.txt.gpg \
    ~/.dais/pds-password.txt
```

### 4. Backup Configuration

```bash
# Backup config
cp ~/.dais/config.toml ~/dais-backup/

# Backup entire .dais directory (excluding backups)
tar -czf ~/dais-backup/dais-config.tar.gz \
    --exclude='backups' \
    ~/.dais/
```

### 5. Backup R2 Media (Future)

```bash
# When media uploads are implemented
wrangler r2 object list dais-media --remote

# Download all media
# (Use rclone or custom script)
```

---

## Restore Procedures

### Full System Restore

**Scenario:** New machine, fresh Cloudflare account, restore everything

**Steps:**

1. **Install Dais CLI**
   ```bash
   git clone https://github.com/yourusername/dais.git
   cd dais
   cargo run --manifest-path client/Cargo.toml -- --help
   ```

2. **Restore Configuration**
   ```bash
   mkdir -p ~/.dais

   # Verify and extract backup
   DAIS_BACKUP_PASSPHRASE_FILE=~/.dais/backup-passphrase \
     scripts/verify-backup-archive.sh ~/.dais/backups/dais_production_backup_YYYYMMDDTHHMMSSZ.tar.gz.gpg
   mkdir -p /tmp/dais-restore
   DAIS_BACKUP_PASSPHRASE_FILE=~/.dais/backup-passphrase \
     gpg --decrypt ~/.dais/backups/dais_production_backup_YYYYMMDDTHHMMSSZ.tar.gz.gpg | tar -xzf - -C /tmp/dais-restore

   # Restore config
   cp /tmp/dais-restore/local/dais/config.toml ~/.dais/
   ```

3. **Restore Keys**
   ```bash
   mkdir -p ~/.dais/keys

   # Restore private key
   cp /tmp/dais-restore/local/dais/keys/private.pem ~/.dais/keys/
   cp /tmp/dais-restore/local/dais/keys/public.pem ~/.dais/keys/

   # Set permissions
   chmod 600 ~/.dais/keys/private.pem
   chmod 644 ~/.dais/keys/public.pem
   ```

4. **Restore PDS Password**
   ```bash
   cp /tmp/dais-restore/local/dais/pds-password.txt ~/.dais/
   chmod 600 ~/.dais/pds-password.txt
   ```

5. **Deploy Infrastructure**
   ```bash
   # Create new D1 database and R2 bucket
   dais deploy infrastructure

   # Note: This creates NEW database ID
   # Update config with new database ID
   ```

6. **Restore Database Data**
   ```bash
   # Import SQL backup to new D1 database
   cd platforms/cloudflare/workers/router
   wrangler d1 execute DB --remote --env production --file=/tmp/dais-restore/database.sql

   # Verify import
   wrangler d1 execute DB --remote --env production --command "SELECT COUNT(*) FROM posts"
   ```

7. **Deploy Workers**
   ```bash
   # Upload secrets
   dais deploy secrets

   # Deploy all workers
   dais deploy workers

   # Verify deployment
   dais doctor
   ```

8. **Verify Restoration**
   ```bash
   # Test endpoints
   dais test webfinger
   dais test actor

   # Check followers
   dais followers list

   # Launch TUI
   dais tui
   ```

### Partial Restore Scenarios

**Scenario 1: Restore Just Database**

```bash
# Download backup
cd platforms/cloudflare/workers/router
wrangler d1 backup download DB <backup-id> --remote --env production --output restore.sql

# Import to existing database
wrangler d1 execute DB --remote --env production --file=restore.sql
```

**Scenario 2: Restore Lost Private Key**

```bash
# Decrypt backup
gpg --decrypt private.pem.gpg > ~/.dais/keys/private.pem

# Set permissions
chmod 600 ~/.dais/keys/private.pem

# Redeploy workers to upload new key
dais deploy secrets
```

**Scenario 3: Restore Configuration After Corruption**

```bash
# Extract config from backup
tar -xzf dais-backup.tar.gz config.toml

# Copy to correct location
cp config.toml ~/.dais/

# Verify
dais doctor
```

---

## Disaster Recovery

### Scenario 1: Accidental Database Deletion

**Problem:** Accidentally deleted all posts or entire database

**Recovery:**

```bash
# 1. List available backups
wrangler d1 backup list DB --remote

# 2. Download most recent backup
wrangler d1 backup download DB <backup-id> --remote --output recovery.sql

# 3. Check what's in backup
head -100 recovery.sql

# 4. Restore
wrangler d1 execute DB --remote --file=recovery.sql

# 5. Verify
wrangler d1 execute DB --remote --command "SELECT COUNT(*) FROM posts"
```

**Data Loss:** Up to 24 hours (if daily backups)

### Scenario 2: Lost Private Key

**Problem:** Private key file deleted, HTTP signatures failing

**Recovery:**

```bash
# Option A: Restore from backup
gpg --decrypt ~/dais-backup/private.pem.gpg > ~/.dais/keys/private.pem
chmod 600 ~/.dais/keys/private.pem
dais deploy secrets

# Option B: Generate new key pair (BREAKS FEDERATION)
dais setup init --regenerate-keys
# ⚠️ WARNING: All remote servers must re-fetch your public key
#    Some servers may cache old key for days/weeks
```

**Impact:**
- Option A: No impact (seamless)
- Option B: Federation broken until remote servers refresh public key

### Scenario 3: Cloudflare Account Locked/Deleted

**Problem:** Lost access to Cloudflare account

**Recovery:**

```bash
# 1. Create new Cloudflare account
# 2. Restore from backup to new account
# 3. Update DNS to point to new workers
# 4. Federation will break temporarily (new URLs)

# Full restore procedure:
dais setup init  # New Cloudflare credentials
dais deploy all  # Deploy to new account

# Import database backup
wrangler d1 execute DB --remote --file=backup.sql

# Update DNS records (at registrar)
# - Point dais.social to new Cloudflare Workers
# - Update social.dais.social
# - Update pds.dais.social
```

**Data Loss:** None (if backup exists)
**Downtime:** 1-4 hours (DNS propagation)

### Scenario 4: Corrupted Database

**Problem:** Database corrupted, queries failing

**Recovery:**

```bash
# 1. Create new database
wrangler d1 create dais-db-recovery --remote

# 2. Update config with new database ID
# 3. Restore from backup
wrangler d1 execute dais-db-recovery --remote --file=backup.sql

# 4. Update workers to use new database
dais deploy workers

# 5. Delete old corrupted database
wrangler d1 delete DB --remote
```

---

## Backup Schedule Recommendations

### Production Instance (Public-facing)

**Daily backups:**
- D1 database
- Configuration files
- Keys

**Weekly backups:**
- R2 media (when implemented)
- Worker code snapshots

**Monthly backups:**
- Full system snapshot
- Archive to cold storage

**Retention:**
- Daily backups: 30 days
- Weekly backups: 90 days
- Monthly backups: 1 year

### Personal Instance (Low traffic)

**Weekly backups:**
- D1 database
- Keys and config

**Retention:**
- Keep last 4 backups (1 month)

---

## Backup Storage Recommendations

### Local Storage

**Pros:**
- Fast restore
- Full control
- No cloud costs

**Cons:**
- Vulnerable to hardware failure
- No off-site protection
- Manual management

**Recommendation:** External hard drive, encrypted

### Cloud Storage (Encrypted)

**Recommended services:**
1. **Backblaze B2** - $0.005/GB/month
2. **AWS S3 Glacier** - $0.004/GB/month
3. **Google Cloud Storage Nearline** - $0.01/GB/month

**Upload script:**

```bash
#!/bin/bash
# Upload encrypted backup to Backblaze B2

BACKUP_FILE="$1"

# Install B2 CLI: pip install b2
b2 upload-file dais-backups "${BACKUP_FILE}.gpg" \
    "backups/$(basename ${BACKUP_FILE}.gpg)"

echo "✓ Uploaded to B2: ${BACKUP_FILE}.gpg"
```

### Git Repository (Config Only)

**For configuration only** (NOT private keys!):

```bash
# Create private backup repo
git init ~/.dais-backup
cd ~/.dais-backup

# Copy config (NOT keys!)
cp ~/.dais/config.toml .

# Commit and push
git add config.toml
git commit -m "Backup config $(date)"
git push origin main
```

**Never commit:**
- `private.pem`
- `pds-password.txt`
- `config.toml` (if contains API tokens)

---

## Backup Verification

### Test Restore Quarterly

**Procedure:**

1. Create test instance
2. Restore from backup
3. Verify all data present
4. Test functionality
5. Destroy test instance

**Checklist:**
- [ ] Database restored completely
- [ ] All posts present
- [ ] Followers list intact
- [ ] Keys working (HTTP signatures valid)
- [ ] PDS authentication works
- [ ] TUI displays data correctly

### Automated Verification

```bash
#!/bin/bash
# Verify backup integrity

BACKUP_FILE="$1"

scripts/verify-backup-archive.sh "$BACKUP_FILE"
```

---

## Best Practices

### 1. Encrypt Sensitive Backups

Always encrypt backups containing:
- Private keys
- Passwords
- API tokens

Use strong passphrases:
```bash
# Generate strong passphrase
openssl rand -base64 32
```

### 2. Store Backups Off-site

**3-2-1 Backup Rule:**
- **3** copies of data
- **2** different media types (local + cloud)
- **1** copy off-site

### 3. Test Restores Regularly

**Monthly test restore** ensures:
- Backups are not corrupted
- Restore procedure works
- Documentation is up to date

### 4. Monitor Backup Success

**Setup notifications:**

```bash
# Email on backup failure
./backup.sh || echo "Backup failed!" | mail -s "Dais Backup Failed" admin@example.com
```

### 5. Document Recovery Procedures

Keep printed copy of restore procedures in case of:
- Computer failure
- Loss of access to digital documentation

---

## Support

**Issues:** https://github.com/yourusername/dais/issues
**Backup Script:** `scripts/backup.sh`
**Restore Help:** See `OPERATIONAL_RUNBOOK.md`

---

**Backup regularly, test restores, sleep soundly! 💤**
