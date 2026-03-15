# Dais TUI - Keyboard Shortcuts Reference

**Quick reference for keyboard navigation in the Dais Terminal UI**

---

## Global Shortcuts (Available Everywhere)

| Key | Action | Description |
|-----|--------|-------------|
| `q` | Quit | Exit the TUI application |
| `d` | Dashboard | Return to main dashboard |
| `n` | New Post | Open composer to create new post |
| `f` | Followers | View and manage followers |
| `m` | Moderation | Review pending replies |
| `b` | Blocks | Manage blocked users/domains |
| `i` | Direct Messages | ActivityPub private messages |
| `x` | Bluesky Chats | Bluesky chat conversations |
| `?` | Help | Show help screen |

---

## Dashboard Screen

| Key | Action |
|-----|--------|
| `r` | Refresh stats |
| Navigation | Use global shortcuts above |

**Stats Displayed:**
- Total posts
- Total followers
- Pending moderation
- Unread notifications

---

## Composer Screen (New Post)

| Key | Action | Description |
|-----|--------|-------------|
| `Ctrl+S` | Send | Publish the post |
| `Ctrl+C` | Cancel | Cancel and return to dashboard |
| `Tab` | Next Field | Move between text/protocol/visibility |
| `Shift+Tab` | Previous Field | Move back between fields |
| `Escape` | Cancel | Same as Ctrl+C |

**Protocol Options:**
1. ActivityPub only
2. Bluesky (AT Protocol)
3. Both (dual-protocol)

**Visibility Options:**
1. Public
2. Unlisted
3. Followers-only
4. Direct message

---

## Followers Screen

| Key | Action | Description |
|-----|--------|-------------|
| `r` | Refresh | Reload follower list |
| `a` | Approve | Approve selected follower request |
| `x` | Reject | Reject selected follower request |
| `Enter` | View Details | Show follower information |
| `↑` / `↓` | Navigate | Move between followers |
| `Escape` | Back | Return to previous screen |

**Follower States:**
- `pending` - Awaiting your approval
- `accepted` - Approved follower
- `rejected` - Blocked follower request

---

## Moderation Screen

| Key | Action | Description |
|-----|--------|-------------|
| `r` | Refresh | Reload moderation queue |
| `a` | Approve | Approve selected reply (makes visible) |
| `x` | Reject | Reject selected reply (hides from threads) |
| `h` | Hide/Unhide | Toggle reply visibility |
| `f` | Filter | Cycle through pending/approved/rejected |
| `Enter` | View Details | Show full reply content |
| `↑` / `↓` | Navigate | Move between replies |
| `Escape` | Back | Return to previous screen |

**Moderation Statuses:**
- `pending` - Default state, needs review
- `approved` - Visible in public threads
- `rejected` - Hidden from threads
- `auto_approved` - Passed automated filters

**Moderation Score:**
- 0.0 = Safe
- 0.5-0.8 = Suspicious
- 0.9+ = Likely spam

---

## Blocks Screen

| Key | Action | Description |
|-----|--------|-------------|
| `r` | Refresh | Reload block list |
| `d` | Delete Block | Remove selected block |
| `Enter` | View Details | Show block information |
| `↑` / `↓` | Navigate | Move between blocks |
| `Escape` | Back | Return to previous screen |

**Block Types:**
- `user` - Blocked individual user
- `domain` - Blocked entire instance

---

## Direct Messages (ActivityPub)

| Key | Action | Description |
|-----|--------|-------------|
| `r` | Refresh | Reload conversation list |
| `n` | New DM | Start new direct message |
| `b` | Switch to Bluesky | Go to Bluesky Chats screen |
| `Enter` | Open Conversation | View message thread |
| `Ctrl+S` | Send Message | Send typed message (in conversation) |
| `↑` / `↓` | Navigate | Move between conversations |
| `Escape` | Back | Return to previous screen / Exit conversation |

**Protocol:** ActivityPub `Note` with `visibility: direct`

---

## Bluesky Chats

| Key | Action | Description |
|-----|--------|-------------|
| `r` | Refresh | Reload chat conversations |
| `a` | Switch to ActivityPub | Go to ActivityPub DMs screen |
| `n` | New Chat | Start new Bluesky conversation |
| `Enter` | Open Chat | View message thread |
| `Ctrl+S` | Send Message | Send typed message (in chat) |
| `↑` / `↓` | Navigate | Move between conversations |
| `Escape` | Back | Return to previous screen / Exit chat |

**Protocol:** `chat.bsky.convo.*` (separate from ActivityPub)

**Unread Indicator:** Red dot `●` shows unread messages

---

## Thread Viewer

| Key | Action | Description |
|-----|--------|-------------|
| `r` | Refresh | Reload thread |
| `↑` / `↓` | Scroll | Navigate through thread |
| `Page Up` / `Page Down` | Fast Scroll | Jump through longer threads |
| `Escape` | Back | Return to previous screen |

**Display:**
- Original post at top
- Replies below (both ActivityPub + Bluesky)
- Protocol indicators: `[🦋 AP]` or `[☁️ AT]` or `[🦋 AP + AT]`

---

## Notifications Screen

| Key | Action | Description |
|-----|--------|-------------|
| `r` | Refresh | Reload notifications |
| `m` | Mark Read | Mark selected notification as read |
| `Enter` | View Details | Open related post/reply |
| `↑` / `↓` | Navigate | Move between notifications |
| `Escape` | Back | Return to previous screen |

**Notification Types:**
- `follow` - New follower
- `mention` - Mentioned in post
- `reply` - Reply to your post
- `like` - Post liked (future)
- `repost` - Post reposted (future)

---

## Text Input Fields

| Key | Action |
|-----|--------|
| `Ctrl+A` | Select all |
| `Ctrl+E` | Move to end of line |
| `Ctrl+U` | Clear line |
| `Ctrl+W` | Delete word backward |
| `Home` | Move to start of line |
| `End` | Move to end of line |
| `Backspace` | Delete character backward |
| `Delete` | Delete character forward |

---

## Tips & Tricks

### Fast Navigation
- Use global shortcuts (`d`, `f`, `m`, etc.) from any screen
- Press `Escape` to go back one level
- Press `q` to quit immediately

### Protocol Switching
- `a` in Bluesky Chats → Switch to ActivityPub DMs
- `b` in ActivityPub DMs → Switch to Bluesky Chats
- Seamless switching between messaging protocols

### Moderation Workflow
1. Press `m` to open Moderation
2. Press `f` to filter by `pending`
3. Use `↑`/`↓` to navigate
4. Press `a` to approve or `x` to reject
5. Press `r` to refresh queue

### Creating Dual-Protocol Posts
1. Press `n` to open Composer
2. Type your message
3. Tab to Protocol selector
4. Select "Both"
5. Ctrl+S to send
6. Post appears on ActivityPub AND Bluesky

### Privacy Protection
- Followers-only posts automatically blocked from Bluesky
- Warning message shown when protocol downgraded
- Database updated to reflect actual delivery

---

## Command Line Alternatives

All TUI actions can also be performed via CLI:

```bash
# Post
dais post create "Hello!" --protocol both

# Followers
dais followers list
dais followers approve @user@domain.com

# Blocks
dais block user @spammer@domain.com
dais block list

# Direct Messages
dais dm send @user@domain.com "Hello!"
```

---

## Customization

**Config Location:** `~/.dais/config.toml`

**Keyboard Shortcuts:** Currently hardcoded, future versions may support customization

**Theme:** TUI uses `THEME` variable from worker config (default: `cat-light`)

---

**For more help, press `?` in the TUI or run `dais --help` in terminal.**
