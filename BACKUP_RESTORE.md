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

```bash
#!/bin/bash
# Daily backup script for Dais instance

set -e  # Exit on error

# Configuration
BACKUP_DIR="${HOME}/.dais/backups"
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="${BACKUP_DIR}/dais_backup_${DATE}.tar.gz"

# Create backup directory
mkdir -p "${BACKUP_DIR}"

# Temporary directory for backup
TEMP_DIR=$(mktemp -d)
trap "rm -rf ${TEMP_DIR}" EXIT

echo "=== Dais Backup - ${DATE} ==="
echo

# 1. Backup D1 Database
echo "[1/5] Backing up D1 database..."
cd workers/actor
wrangler d1 backup create DB --remote > "${TEMP_DIR}/db_backup_id.txt"
BACKUP_ID=$(cat "${TEMP_DIR}/db_backup_id.txt" | grep -oP 'Backup ID: \K[a-f0-9-]+')
wrangler d1 backup download DB "${BACKUP_ID}" --remote --output "${TEMP_DIR}/database.sql"
echo "  ✓ Database backed up (${BACKUP_ID})"

# 2. Backup Keys
echo "[2/5] Backing up cryptographic keys..."
cp ~/.dais/keys/private.pem "${TEMP_DIR}/private.pem"
cp ~/.dais/keys/public.pem "${TEMP_DIR}/public.pem"
echo "  ✓ Keys backed up"

# 3. Backup PDS Password
echo "[3/5] Backing up PDS password..."
cp ~/.dais/pds-password.txt "${TEMP_DIR}/pds-password.txt"
echo "  ✓ PDS password backed up"

# 4. Backup Configuration
echo "[4/5] Backing up configuration..."
cp ~/.dais/config.toml "${TEMP_DIR}/config.toml"
echo "  ✓ Configuration backed up"

# 5. Create encrypted archive
echo "[5/5] Creating encrypted backup archive..."
tar -czf "${TEMP_DIR}/backup.tar.gz" -C "${TEMP_DIR}" \
    database.sql \
    private.pem \
    public.pem \
    pds-password.txt \
    config.toml \
    db_backup_id.txt

# Encrypt with GPG (optional - recommended for cloud storage)
if command -v gpg &> /dev/null; then
    gpg --symmetric --cipher-algo AES256 \
        --output "${BACKUP_FILE}.gpg" \
        "${TEMP_DIR}/backup.tar.gz"
    echo "  ✓ Backup encrypted: ${BACKUP_FILE}.gpg"
else
    mv "${TEMP_DIR}/backup.tar.gz" "${BACKUP_FILE}"
    echo "  ⚠ Backup NOT encrypted (install gpg for encryption): ${BACKUP_FILE}"
fi

echo
echo "✓ Backup complete!"
echo "  File: ${BACKUP_FILE}$([ -f ${BACKUP_FILE}.gpg ] && echo .gpg || echo '')"
echo "  Size: $(du -h ${BACKUP_FILE}$([ -f ${BACKUP_FILE}.gpg ] && echo .gpg || echo '') | cut -f1)"

# Cleanup old backups (keep last 30 days)
echo
echo "Cleaning up old backups (keeping last 30 days)..."
find "${BACKUP_DIR}" -name "dais_backup_*.tar.gz*" -mtime +30 -delete
echo "✓ Cleanup complete"
```

**Usage:**
```bash
chmod +x scripts/backup.sh
./scripts/backup.sh
```

### Automated Backup with Cron

**Setup daily backup at 2 AM:**

```bash
# Edit crontab
crontab -e

# Add this line:
0 2 * * * /home/user/Projects/dais/scripts/backup.sh >> /home/user/.dais/backups/backup.log 2>&1
```

---

## Manual Backup Procedures

### 1. Backup D1 Database

**Method A: Using wrangler CLI**

```bash
# Create backup
cd workers/actor
wrangler d1 backup create DB --remote

# List backups
wrangler d1 backup list DB --remote

# Download specific backup
wrangler d1 backup download DB <backup-id> --remote --output database.sql
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
   pip install -e cli/
   ```

2. **Restore Configuration**
   ```bash
   mkdir -p ~/.dais

   # Extract backup
   gpg --decrypt ~/dais-backup.tar.gz.gpg | tar -xzf - -C /tmp/

   # Restore config
   cp /tmp/config.toml ~/.dais/
   ```

3. **Restore Keys**
   ```bash
   mkdir -p ~/.dais/keys

   # Restore private key
   cp /tmp/private.pem ~/.dais/keys/
   cp /tmp/public.pem ~/.dais/keys/

   # Set permissions
   chmod 600 ~/.dais/keys/private.pem
   chmod 644 ~/.dais/keys/public.pem
   ```

4. **Restore PDS Password**
   ```bash
   cp /tmp/pds-password.txt ~/.dais/
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
   cd workers/actor
   wrangler d1 execute DB --remote --file=/tmp/database.sql

   # Verify import
   wrangler d1 execute DB --remote --command "SELECT COUNT(*) FROM posts"
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
wrangler d1 backup download DB <backup-id> --remote --output restore.sql

# Import to existing database
cd workers/actor
wrangler d1 execute DB --remote --file=restore.sql
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

# Decrypt
gpg --decrypt "${BACKUP_FILE}.gpg" > /tmp/test_backup.tar.gz

# Extract
tar -tzf /tmp/test_backup.tar.gz

# Check required files
REQUIRED_FILES=(
    "database.sql"
    "private.pem"
    "public.pem"
    "pds-password.txt"
    "config.toml"
)

for file in "${REQUIRED_FILES[@]}"; do
    if tar -tzf /tmp/test_backup.tar.gz | grep -q "$file"; then
        echo "✓ $file present"
    else
        echo "✗ $file MISSING"
        exit 1
    fi
done

# Cleanup
rm /tmp/test_backup.tar.gz

echo "✓ Backup verification complete"
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
