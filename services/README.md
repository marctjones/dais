# Dais Services

There are no active Python sidecar services in the current dais architecture.

The former `bluesky_reply_consumer.py` sidecar has been retired. It targeted the
old Python/legacy worker layout and did not have production-grade CAR block
decoding, commit record extraction, DID/handle resolution, or reply-parent
matching tests. Keeping it in the active tree made Bluesky reply ingestion look
more complete than it is.

Current AT Protocol and Bluesky work belongs in the Rust core/router path:

- `core/` for shared ATProto record, repository, and protocol behavior.
- `platforms/cloudflare/workers/router/` for deployed PDS/AppView-compatible
  endpoints.
- `client/` and `client-core/` for first-party owner/client workflows.

Do not add a new sidecar service for Bluesky reply ingestion unless a GitHub
issue under epic #70 defines the ownership model, tests, deployment target, and
release gate. For v1.31, track active ATProto/Bluesky work in issues #275,
#276, #277, and #278.
