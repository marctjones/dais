# Migration And Import Tools

Dais import work starts with owner-reviewable plans. The tool must show what it
will change before it follows people, adds watches/sources, or recreates posts.

## Preview

Use `scripts/import-preview.rb` to convert owner-supplied exports into a JSON
plan:

```bash
scripts/import-preview.rb --format opml --file subscriptions.opml --output import-plan.json
scripts/import-preview.rb --format mastodon-following-csv --file following_accounts.csv
scripts/import-preview.rb --format bluesky-follows --file bluesky-handles.json
scripts/import-preview.rb --format local-posts-json --file posts.json
```

Supported formats:

| Format | Input | Planned action |
| --- | --- | --- |
| `opml` | OPML with `outline xmlUrl` entries | `owner source-add rss/atom` |
| `rss-list` | one feed URL per line, optional tab-separated title | `owner source-add rss` |
| `mastodon-following-csv` | Mastodon following CSV export | `owner follow @user@domain` |
| `bluesky-follows` | JSON/text/CSV list of handles or DIDs | `owner watch-add bluesky_actor` |
| `bluesky-starter-pack` | owner-supplied JSON handles/members export | `owner watch-add bluesky_actor` |
| `local-posts-json` | JSON array or object with `items/posts/orderedItems` | `owner post-create` |
| `mastodon-outbox-json` | ActivityPub outbox-style JSON | `owner post-create` |

Bluesky imports currently use private watches because Dais supports public
Bluesky actor monitoring but does not treat private Dais relationships as public
ATProto follows by default.

## Apply

Apply only after reviewing the generated plan:

```bash
scripts/import-preview.rb \
  --format opml \
  --file subscriptions.opml \
  --instance-url https://social.dais.social \
  --owner-token-file /secure/path/owner-token \
  --apply
```

The apply path uses the Rust owner CLI and the live owner API. Owner tokens are
passed through environment variables to avoid printing them in the JSON plan.

## Boundaries

- Import does not bypass private-by-default posting.
- Imported RSS/OPML sources are private reader sources.
- Imported Bluesky actors are watches unless a future public-follow flow is
  explicitly added.
- Local archive posts are restored as followers-only unless the input supplies a
  supported visibility.
- The preview/apply split is required for managed support and self-hosted use.
