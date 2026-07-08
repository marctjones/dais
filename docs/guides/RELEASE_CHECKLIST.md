# v1.0.0 Release Checklist

## ✅ Completed

- [x] Clean up session summary files
- [x] Update README.md with all features
- [x] Create FEATURES.md with 200+ features
- [x] Create INSTALL.md with installation guide
- [x] Create CHANGELOG.md with version history
- [x] Update pyproject.toml version to 1.0.0
- [x] Stage all changes
- [x] Create release commit
- [x] Create v1.0.0 git tag
- [x] Create RELEASE_NOTES_v1.0.0.md for GitHub

## 🚀 Next Steps (Do These Now)

### 1. Push to GitHub

```bash
# Push commits
git push origin main

# Push tag
git push origin v1.0.0
```

### 2. Create GitHub Release

1. Go to: https://github.com/yourusername/dais/releases/new
2. Select tag: `v1.0.0`
3. Release title: `v1.0.0: Stable Cloudflare Edition`
4. Copy content from `RELEASE_NOTES_v1.0.0.md` into description
5. Check "Set as the latest release"
6. Click "Publish release"

### 3. Verify Release

Check these work:
- [ ] Release page shows correctly on GitHub
- [ ] Tag `v1.0.0` is visible
- [ ] Release notes display properly
- [ ] All documentation links work
- [ ] CHANGELOG.md link works

### 4. Announce (Optional)

Post on:
- [ ] Fediverse (`@social@dais.social`)
- [ ] Bluesky (`@social.dais.social`)
- [ ] GitHub Discussions
- [ ] Your personal social media

Example announcement:
```
🎉 dais v1.0.0 is released!

Run your own single-user ActivityPub + Bluesky server on Cloudflare (free tier).

✨ 200+ features
🖥️  Terminal UI
🔒 Cloudflare Access auth
💰 $0/month hosting

Install: cargo run --manifest-path client/Cargo.toml -- --help
Deploy: dais deploy all

https://github.com/yourusername/dais/releases/tag/v1.0.0
```

## 📋 Post-Release

### Create Next Release Branch

```bash
# Create and checkout a branch for the next focused milestone
git checkout -b feature/<short-name>

# Push branch to GitHub
git push -u origin feature/<short-name>
```

This branch will be for:
- A focused release-sized improvement
- Tests and conformance checks for the changed behavior
- Documentation updates for user-visible changes

### Update Issues

- [ ] Close completed issues
- [ ] Create follow-up issues for release-blocking bugs
- [ ] Create the next incremental milestone when needed
- [ ] Assign issues to the next incremental release milestone

### Documentation

- [ ] Update GitHub repository description
- [ ] Update repository topics/tags
- [ ] Add screenshot to README (optional)
- [ ] Update social preview image (optional)

## 🐛 Monitor

After release, watch for:
- [ ] Installation issues
- [ ] Deployment failures
- [ ] Documentation gaps
- [ ] Bug reports

Create a patch release if critical bugs are found.

---

## Quick Commands Reference

```bash
# Check current status
git status
git log --oneline -5
git tag -l

# Push everything
git push origin main
git push origin v1.0.0

# Create new branch
git checkout -b feature/<short-name>
git push -u origin feature/<short-name>

# View release
git show v1.0.0
```

## Current Release Evidence Flow

For current Cloudflare-backed releases, use the server release script as the
canonical deploy gate:

```bash
scripts/release-server.sh --deploy --strict --bluesky-conformance --mastodon-conformance
```

For GUI releases, use:

```bash
scripts/release-desk-v2.sh
```

Package and upload release evidence after creating a GitHub Release:

```bash
scripts/publish-release-evidence.sh --tag vX.Y.Z --report-dir tmp/server-release-YYYYMMDDTHHMMSSZ
```

Use `--dry-run` to create the archive without uploading it. Do not upload
reports containing owner tokens, private keys, passphrases, decrypted private
messages, or other secret material.
