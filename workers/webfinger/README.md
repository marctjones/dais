# WebFinger Worker

WebFinger endpoint for dais.social ActivityPub server.

## Endpoint

`GET https://dais.social/.well-known/webfinger?resource=acct:marc@dais.social`

## Response

Returns a JSON Resource Descriptor (JRD) that maps the account identifier to the ActivityPub actor URL.

```json
{
  "subject": "acct:marc@dais.social",
  "aliases": [
    "https://social.dais.social/users/marc"
  ],
  "links": [
    {
      "rel": "self",
      "type": "application/activity+json",
      "href": "https://social.dais.social/users/marc"
    },
    {
      "rel": "http://webfinger.net/rel/profile-page",
      "type": "text/html",
      "href": "https://dais.social/@marc"
    }
  ]
}
```

## Development

```bash
# Install worker-build
cargo install worker-build

# Build and run locally
wrangler dev

# Test
curl "http://localhost:8787/.well-known/webfinger?resource=acct:marc@dais.social"

# Deploy to production
wrangler deploy
```

## References

- [RFC 7033: WebFinger](https://tools.ietf.org/html/rfc7033)
- [Cloudflare Workers Rust](https://developers.cloudflare.com/workers/languages/rust/)
