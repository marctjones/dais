# Design: Dais Desk Interaction and Visual System

**Status:** Accepted design target for issue #173.
**Scope:** Reusable interaction rules and UI primitives for the first-party Dais
Desk GUI client.
**Related:** `docs/design/DAIS_DESK_PRODUCT_UX.md`,
`docs/design/DAIS_DESK_INFORMATION_ARCHITECTURE.md`, `docs/POSITIONING.md`,
`docs/design/PRIVATE_MODE.md`,
`docs/guides/DAIS_DESK_APP.md`.

## 1. Design Posture

Dais Desk is daily social software for a privacy-seeking owner/operator. The UI
should be calm, dense, and legible. It should feel closer to Mail, Finder, and a
professional feed reader than to a marketing site or an engagement-optimized
social network.

The interaction system optimizes for:

- Fast scanning of posts, people, and server state.
- Clear consequences before public sharing or relationship changes.
- Keyboard-first operation with mouse/touch parity.
- Stable layout under long names, handles, URLs, counters, media, and warnings.
- Privacy state visible without making the interface feel alarmist.

## 2. Layout Primitives

### 2.1 App Shell

The shell has four persistent regions:

- **Source list**: Home, People, and Server modes, with secondary screens inside
  the active mode.
- **Toolbar**: account switcher, command/search field, compose button, refresh,
  and attention indicator.
- **Primary content**: feed, relationship list, or operator list.
- **Inspector**: selected post, person, source, delivery, moderation item, or
  server diagnostic.

Desktop target layout:

| Region | Target size | Constraints |
| --- | --- | --- |
| Source list | 220-260 px | Fixed while window resizes; collapses below narrow breakpoint. |
| Toolbar | 48-56 px tall | Never wraps primary actions; overflow moves into menu. |
| Primary content | Flexible | Keeps list row widths stable and scrolls independently. |
| Inspector | 340-440 px | Collapsible; selected item remains in list when collapsed. |

Narrow layout:

- Source list becomes a mode switcher plus screen menu.
- Inspector opens as a pushed detail view, not an overlapping card.
- Compose remains a sheet, but fills most of the viewport.

### 2.2 Lanes

Home lanes group daily social work: Friends, Following, Mentions, DMs, Watches,
Saved, Drafts, and My Posts.

Lane rules:

- Lane headers stay 32-36 px tall.
- Counters reserve a fixed width so counts do not shift labels.
- Empty lanes show one concise empty state and the next available action.
- Loading, error, and stale states occupy the same footprint as loaded rows.

### 2.3 Inspector

The inspector shows consequences and details for the selected object.

Inspector header:

- Title or handle.
- Relationship or visibility chip.
- Primary action group.
- More menu for uncommon actions.

Inspector body:

- Summary first, raw protocol details last.
- Delivery and moderation sections collapse by default unless they need action.
- Copyable diagnostic evidence is available only in Server-facing contexts.

## 3. Components

### 3.1 Source List Item

Use for mode and screen navigation.

Required parts:

- Icon or compact symbol.
- Text label.
- Optional count badge.
- Optional attention marker.

Rules:

- Row height: 30-34 px.
- Label truncates with ellipsis.
- Count badge has a fixed minimum width and tabular numerals.
- Active state uses background, leading marker, and text weight, not color alone.

### 3.2 Feed Row

Use for posts, replies, mentions, DMs, saved items, and watched public posts.

Required parts:

- Avatar or source icon.
- Author display name and handle/source.
- Timestamp.
- Visibility chip.
- Relationship chip when relevant.
- Content excerpt.
- Action summary: replies, favorites, boosts/reposts, bookmarks, warnings.

Rules:

- Minimum row height: 92 px for posts, 72 px for compact notifications.
- Avatar size: 36 px in lists, 48 px in inspector.
- Excerpt clamps to three lines in lists.
- Media thumbnails use fixed aspect-ratio containers.
- Long URLs break safely without expanding the row horizontally.
- Selected state keeps row height unchanged.

### 3.3 Post Card

Use in thread, detail, saved, and My Posts surfaces.

Required parts:

- Author/source block.
- Body.
- Media block if present.
- Visibility and route summary.
- Reply/repost/favorite/bookmark actions.
- Delivery/moderation warning area when needed.

Rules:

- Card radius: 6-8 px.
- Cards are not nested inside other cards.
- Warning area is reserved below the body and does not cover content.
- Public, followers-only, direct, encrypted, and watch-only states use distinct
  labels and icons.

### 3.4 Person and Source Card

Use for accounts, people, watched sources, and discovery results.

Required parts:

- Avatar/source icon.
- Display name/title.
- Handle, URL, or domain.
- Relationship state.
- Trust/source evidence.
- Primary action: Follow, Friend, Watch, Message, Block, or Open.
- Secondary actions in a menu.

Rules:

- Relationship state answers: do I see them, do they see me, do they know, can
  they see private posts, can I DM them, and are they muted or blocked.
- Follow and Watch are visually different actions.
- Watch states never imply a remote relationship.
- Official/trust evidence is shown as provenance, not as a central-platform
  verification badge.

### 3.5 Chips

Chips are compact state labels, not generic decoration.

Required chip families:

- **Visibility**: Public, Followers, Friends, Direct, Encrypted, Draft.
- **Relationship**: Friend, Follower, Following, Pending, Watch, Muted, Blocked.
- **Moderation**: Needs Review, Hidden, Rejected, Approved, AI Advisory.
- **Delivery**: Queued, Delivered, Partial, Failed, Retrying, Cancelled.
- **Source**: RSS, Public Account, Public Post, Saved Search, Local.

Rules:

- Height: 22-24 px.
- Use an icon/symbol plus text for safety-critical chips.
- Use outline or filled treatment consistently by family.
- Never rely on color alone; shape, icon, and text carry meaning.
- Long chip labels truncate only after the meaningful word remains visible.

### 3.6 Buttons and Menus

Button hierarchy:

- Primary: default safe action for the current screen.
- Secondary: common non-destructive actions.
- Destructive: delete, revoke, block, reject, remove token.
- Disclosure/menu: secondary or advanced actions.

Rules:

- Minimum height: 28 px desktop, 36 px touch/narrow.
- Icon-only buttons require tooltip and accessible label.
- Destructive buttons use text, icon, and confirmation where remote or
  irreversible effects exist.
- Menus group actions by consequence: safe, visibility-changing, destructive.

### 3.7 Forms

Use forms for compose, profile, settings, audience groups, moderation policy,
tokens, and watch creation.

Rules:

- Field labels are always visible.
- Help text explains consequences, not implementation.
- Validation appears next to the field and in a summary for multi-field forms.
- Save buttons remain disabled until changes are valid.
- Public state changes show a preview before saving.

## 4. Sheets, Dialogs, and Confirmations

### 4.1 Sheets

Use sheets for reversible or draftable work:

- Compose.
- Profile editing.
- Audience group editing.
- Watch creation.
- Relationship changes where the result can be previewed.

Sheet rules:

- Title states the task.
- The first control is the highest-consequence choice: identity, audience,
  relationship type, or source target.
- Escape closes only when no data would be lost, otherwise asks to discard.
- Primary action text reflects consequence: Post Publicly, Post to Friends, Send
  Direct, Watch Public Posts, Approve Follower.

### 4.2 Dialogs

Use dialogs for short decisions and destructive confirmations:

- Delete post.
- Revoke media.
- Block actor/domain.
- Remove follower.
- Rotate or remove token.
- Publish publicly after warning.

Dialog rules:

- Body contains three facts: what changes, who can be affected, and whether it
  can be undone.
- Destructive action is not the default focused button.
- The cancel action is always visible.
- Protocol details are hidden unless the decision depends on them.

## 5. Keyboard and Focus

Global shortcuts:

| Shortcut | Action |
| --- | --- |
| Command-1 | Home |
| Command-2 | People |
| Command-3 | Server |
| Command-K | Command/search |
| Command-N | Compose |
| Command-R | Refresh current view |
| Escape | Close sheet/dialog or return from detail |
| Space | Toggle selection where applicable |
| Return | Open focused item or accept safe default |

List navigation:

- Arrow keys move selection.
- Page Up/Page Down move by viewport.
- Home/End move to first/last item.
- Typeahead filters the active list when no text field is focused.

Focus order:

1. Source list or mode switcher.
2. Toolbar controls.
3. Primary content list.
4. Inspector.
5. Inline action group.
6. Footer/status controls.

Focus rules:

- Visible focus is always present.
- Tab moves between control groups, not every row in a large list.
- Arrow keys move within list-like controls.
- Screen-reader labels include icon-only action names and current state.
- Keyboard and pointer actions trigger the same confirmation flows.

## 6. State and Color

Use system-adjacent colors and semantic tokens instead of one-off colors.

Token families:

- Text: primary, secondary, tertiary, disabled.
- Surface: window, sidebar, content, raised, selected, hover.
- Border: hairline, regular, strong.
- Accent: selected and primary action.
- Success: delivered, approved, healthy.
- Warning: public sharing, partial delivery, sensitive content, stale data.
- Danger: destructive, failed, blocked, rejected.
- Privacy: private/friends/direct/encrypted state.

State rules:

- Success, warning, and danger always combine color with label and icon.
- High-contrast mode increases borders and removes low-contrast fills.
- Dark mode uses separate tokens, not inverted light-mode colors.
- Text contrast meets WCAG AA for normal text.
- Motion is optional and respects reduced-motion settings.

Suggested default tone:

- Neutral chrome and white/near-white content in light mode.
- Dark neutral surfaces in dark mode.
- Blue accent for selection and primary safe action.
- Amber warning for public/sensitive/partial states.
- Red danger for destructive or failed states.
- Green success only for completed/healthy states.

## 7. Stable Dimensions

Stable dimensions are part of the design system because social content is
variable and often hostile to layout.

Rules:

- Source list width is fixed per breakpoint.
- Toolbar height is fixed.
- Row min heights are defined per row type.
- Avatars and source icons have fixed sizes.
- Chips have fixed height and bounded max width.
- Counters use tabular numerals.
- Media uses aspect-ratio boxes.
- Buttons do not resize on hover or active states.
- Loading, error, and empty states reserve the same footprint as content.
- Text never scales with viewport width.
- Long words, handles, and URLs wrap or truncate within their container.

## 8. Responsive Behavior

Desktop:

- Source list, primary content, and inspector can all be visible.
- Inspector can collapse.
- Compose is a centered sheet with bounded width.

Tablet/narrow desktop:

- Source list can collapse to mode icons and active screen menu.
- Inspector becomes a pushed detail panel.
- Toolbar actions move into a menu before wrapping.

Phone-width future target:

- One column at a time.
- Mode switcher stays reachable.
- Compose sheet fills the viewport.
- Destructive confirmations remain full width with clear cancel action.

## 9. Accessibility

Minimum acceptance:

- All workflows can be completed by keyboard.
- Focus order follows visual order.
- Icon-only controls have accessible names and tooltips.
- Status chips expose text to assistive technology.
- Color is never the only indicator.
- Text can scale without overlapping controls.
- Forms announce validation errors.
- Dynamic feed updates do not steal focus.
- Reduced-motion preference is respected.

## 10. Implementation Notes

- Prefer native platform conventions for shortcut names, button ordering, and
  sheet/dialog behavior.
- In the Slint UI, declare accessibility roles, labels, and default actions on
  custom controls instead of relying only on visible text.
- Use a small token set in shared Slint component properties before adding
  component-specific colors.
- Keep component sizing explicit enough that smoke tests can assert dimensions
  and screenshots cannot stretch sparse lists into oversized controls.
- Avoid nested cards and page sections styled as floating cards. Cards are for
  repeated items, modals, and deliberately framed tools.
- Add or adopt an icon library before drawing custom icons by hand. Icons used
  for unfamiliar concepts need text labels or tooltips.

## 11. Acceptance Checklist

- Source list, lanes, inspector, sheets, dialogs, rows, cards, chips, buttons,
  forms, and menus have documented behavior.
- Components have stable dimensions and cannot shift layout on hover, loading,
  selection, warning, or dynamic counts.
- Icons and labels follow platform conventions and are accessible.
- Status, warning, privacy, delivery, moderation, and destructive states are
  distinct by text, icon/shape, and color.
- Light, dark, and high-contrast support can be implemented from semantic tokens.
- Keyboard navigation and focus order are defined for the whole app.
