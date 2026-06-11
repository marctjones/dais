# dais Roadmap

## Deployment Strategy: Option 2

Deploy incrementally with working features, iterate based on real-world usage.

---

## Milestone 1: Alpha Deployment 🚀
**Target:** March 9, 2026 (Tomorrow!)  
**Goal:** Deploy minimal viable alpha - basic posting works

### Critical Issues (Must fix)
- [ ] #33 - Fix: Individual post endpoint returns 404
- [ ] #34 - Fix: dais post create command fails

### Success Criteria
- ✅ People can follow `@social@dais.social`
- ✅ Can create posts via `dais post create`
- ✅ Posts federate to Mastodon/Pleroma followers
- ✅ Outbox serves posts to ActivityPub clients

### Known Limitations (Acceptable for Alpha)
- Posts are JSON-only (no HTML rendering)
- No media attachments
- No replies/likes/boosts
- CLI-only management

### Deployment Checklist
1. Fix #33, #34 (~2 hours)
2. Test locally with `./scripts/test-phase2-local.sh`
3. Deploy workers to Cloudflare
4. Run production migrations
5. Configure DNS
6. Create first post
7. Test from Mastodon
8. Document deployment process (#44)

---

## Milestone 2: Beta Release 🎨
**Target:** March 15, 2026 (1 week)  
**Goal:** Polished features, ready for public promotion

### High Priority Issues
- [ ] #32 - Fix: Inbox OPTIONS endpoint
- [ ] #35 - Fix: Outbox OPTIONS endpoint
- [ ] #36 - Feature: HTML rendering for browsers
- [ ] #37 - Feature: Static landing page
- [ ] #43 - Fix: Test scripts and documentation
- [ ] #44 - Documentation: Production deployment guide

### Success Criteria
- ✅ Posts viewable in web browsers
- ✅ Professional landing page at dais.social
- ✅ All OPTIONS/CORS issues resolved
- ✅ Complete deployment documentation
- ✅ Test suite reliable and documented

### User Experience Improvements
- Share post links on Twitter/HN/Reddit (HTML rendering)
- Landing page explains project
- Clean first impression for visitors

---

## Milestone 3: Production v1.0 🎯
**Target:** March 31, 2026 (3-4 weeks)  
**Goal:** Full-featured single-user ActivityPub server

### Medium Priority Features
- [ ] #38 - Phase 2.5: Media attachments (R2 upload)
- [ ] #39 - Phase 3: Interactions (replies, likes, boosts, DMs)
- [ ] #41 - Phase 4: Management and analytics tools

### Low Priority Enhancements
- [ ] #42 - Feature: Terminal UI (TUI)
- [ ] #40 - Fix: Containerized dev environment

### Success Criteria
- ✅ Full ActivityPub protocol support
- ✅ Media attachments working
- ✅ Two-way interactions (replies, likes, boosts)
- ✅ Direct messages
- ✅ Analytics and reporting
- ✅ Production-ready monitoring

---

## Dependencies

```
Alpha Deployment (Critical Path)
├─ #33 ─┐
└─ #34 ─┴─> Alpha Deploy!
          │
          ├─> Beta Release
          │   ├─ #36 (HTML rendering)
          │   ├─ #37 (Landing page)
          │   ├─ #32, #35 (OPTIONS fixes)
          │   ├─ #43 (Test docs)
          │   └─ #44 (Deployment docs)
          │
          └─> Production v1.0
              ├─ #38 (Media - needs #33, #34)
              ├─ #39 (Interactions)
              ├─ #41 (Management)
              └─ #42, #40 (Nice-to-haves)
```

---

## Priority Labels

- **priority: critical** 🔴 - Must fix for alpha deployment
- **priority: high** 🟠 - Important for beta release
- **priority: medium** 🟡 - Nice to have
- **priority: low** ⚪ - Future enhancement

---

## Timeline Summary

| Milestone | Target | Duration | Focus |
|-----------|--------|----------|-------|
| **Alpha Deployment** | Mar 9 | 1 day | Fix critical bugs, deploy |
| **Beta Release** | Mar 15 | 1 week | Polish UX, documentation |
| **Production v1.0** | Mar 31 | 3-4 weeks | Full features |

---

## Next Steps

1. **Today (Mar 8):** Fix #33, #34
2. **Tomorrow (Mar 9):** Deploy alpha, test federation
3. **This Week:** Beta features (#36, #37)
4. **Next 3 Weeks:** Full v1.0 features

---

## View Progress

```bash
# View all milestones
gh api repos/marctjones/dais/milestones

# View Alpha Deployment issues
gh issue list --milestone "Alpha Deployment"

# View issues by priority
gh issue list --label "priority: critical"
```
