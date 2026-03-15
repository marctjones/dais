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
wrangler d1 backup create DB --remote > "${TEMP_DIR}/db_backup_id.txt" 2>&1 || true
if [ -f "${TEMP_DIR}/db_backup_id.txt" ]; then
    BACKUP_ID=$(cat "${TEMP_DIR}/db_backup_id.txt" | grep -oP 'Backup ID: \K[a-f0-9-]+' || echo "")
    if [ -n "$BACKUP_ID" ]; then
        wrangler d1 backup download DB "${BACKUP_ID}" --remote --output "${TEMP_DIR}/database.sql"
        echo "  ✓ Database backed up (${BACKUP_ID})"
    else
        echo "  ⚠ Could not extract backup ID, using direct export"
        wrangler d1 export DB --remote --output="${TEMP_DIR}/database.sql"
    fi
else
    echo "  ⚠ Backup command failed, using direct export"
    wrangler d1 export DB --remote --output="${TEMP_DIR}/database.sql"
fi
cd ../..

# 2. Backup Keys
echo "[2/5] Backing up cryptographic keys..."
if [ -f ~/.dais/keys/private.pem ]; then
    cp ~/.dais/keys/private.pem "${TEMP_DIR}/private.pem"
    cp ~/.dais/keys/public.pem "${TEMP_DIR}/public.pem"
    echo "  ✓ Keys backed up"
else
    echo "  ⚠ Keys not found, skipping"
fi

# 3. Backup PDS Password
echo "[3/5] Backing up PDS password..."
if [ -f ~/.dais/pds-password.txt ]; then
    cp ~/.dais/pds-password.txt "${TEMP_DIR}/pds-password.txt"
    echo "  ✓ PDS password backed up"
else
    echo "  ⚠ PDS password not found, skipping"
fi

# 4. Backup Configuration
echo "[4/5] Backing up configuration..."
if [ -f ~/.dais/config.toml ]; then
    cp ~/.dais/config.toml "${TEMP_DIR}/config.toml"
    echo "  ✓ Configuration backed up"
else
    echo "  ⚠ Configuration not found, skipping"
fi

# 5. Create encrypted archive
echo "[5/5] Creating backup archive..."
tar -czf "${TEMP_DIR}/backup.tar.gz" -C "${TEMP_DIR}" . 2>/dev/null

# Encrypt with GPG (optional - recommended for cloud storage)
if command -v gpg &> /dev/null; then
    gpg --symmetric --cipher-algo AES256 \
        --batch --yes \
        --passphrase-file <(echo "${DAIS_BACKUP_PASSPHRASE:-changeme}") \
        --output "${BACKUP_FILE}.gpg" \
        "${TEMP_DIR}/backup.tar.gz"
    echo "  ✓ Backup encrypted: ${BACKUP_FILE}.gpg"
    echo "  ⚠ Using passphrase from DAIS_BACKUP_PASSPHRASE env var (or default)"
else
    mv "${TEMP_DIR}/backup.tar.gz" "${BACKUP_FILE}"
    echo "  ⚠ Backup NOT encrypted (install gpg for encryption): ${BACKUP_FILE}"
fi

echo
echo "✓ Backup complete!"
if [ -f "${BACKUP_FILE}.gpg" ]; then
    echo "  File: ${BACKUP_FILE}.gpg"
    echo "  Size: $(du -h ${BACKUP_FILE}.gpg | cut -f1)"
else
    echo "  File: ${BACKUP_FILE}"
    echo "  Size: $(du -h ${BACKUP_FILE} | cut -f1)"
fi

# Cleanup old backups (keep last 30 days)
echo
echo "Cleaning up old backups (keeping last 30 days)..."
find "${BACKUP_DIR}" -name "dais_backup_*.tar.gz*" -mtime +30 -delete 2>/dev/null || true
echo "✓ Cleanup complete"
