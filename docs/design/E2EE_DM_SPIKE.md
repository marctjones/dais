# E2EE DM Spike

Status: recommendation for #71.

## Recommendation

Use MLS over the existing dais ActivityPub transport for decentralized E2EE DMs.
Implement it with OpenMLS in Rust.

This keeps the product architecture simple:

- ActivityPub remains the authenticated delivery transport.
- dais owns the E2EE message format and key discovery extension.
- The protocol adapter layer continues to route public/friends/direct intent
  honestly.
- Mastodon and Bluesky fallback stays explicit: they can receive fallback
  notifications, but they do not become E2EE peers unless they implement the dais
  E2EE extension.

## Why MLS/OpenMLS

RFC 9420 is an IETF standards-track protocol for end-to-end encrypted messaging
with asynchronous group key establishment, forward secrecy, and post-compromise
security. The RFC explicitly targets groups from two members to thousands, which
fits both 1:1 DMs and future small private friend groups:

- RFC 9420: https://www.rfc-editor.org/rfc/rfc9420.html
- OpenMLS book: https://book.openmls.tech/

OpenMLS is a Rust implementation of RFC 9420, exposes high-level group APIs, and
supports a wasm32 build target. That matches the current Rust-first direction and
Cloudflare deployment model better than libsignal bindings or adopting Matrix as
a second social stack.

## Why Not ActivityPub DMs Alone

ActivityPub and Mastodon private/direct posts are audience-scoped delivery, not
confidentiality. Mastodon documents that server administrators of sender and
recipient servers may obtain direct-message text:

- Mastodon posting privacy: https://docs.joinmastodon.org/user/posting/#privacy

So ActivityPub DMs are useful as transport and UX, but not sufficient as the
privacy boundary.

## Why Not Bluesky Chat

Bluesky chat currently uses `chat.bsky.*` APIs and is proxied to the central chat
service DID. That does not satisfy dais decentralized E2EE requirements:

- Bluesky chat API: https://docs.bsky.app/docs/api/chat-bsky-convo-send-message

Bluesky can remain a public-post surface. It should not receive private or E2EE
dais messages unless ATProto later ships a decentralized private-data mechanism
that can express the same guarantees.

## Key Management Design

Publish dais E2EE device material as an ActivityPub actor extension:

```json
{
  "type": "Person",
  "id": "https://social.dais.social/users/social",
  "publicKey": { "...": "existing HTTP signature key" },
  "daisE2ee": {
    "v": 1,
    "protocol": "mls-rfc9420",
    "devices": [
      {
        "deviceId": "primary",
        "credential": "base64url MLS credential",
        "keyPackage": "base64url MLS KeyPackage",
        "createdAt": "2026-06-11T00:00:00Z"
      }
    ]
  }
}
```

Rules:

- Treat actor-document E2EE keys as TOFU until verified.
- Store a fingerprint for every accepted peer device.
- Warn on key changes and require explicit owner approval before sending new
  secrets to a changed device.
- Support safety-number display and QR export in CLI/TUI before calling the UX
  complete.
- Start single-device; add multi-device after the 1:1 lifecycle works.

## Message Format

Use an ActivityPub `Create` activity addressed as a direct message to the
recipient actor. The object is a `Note` with fallback content and a dais E2EE
extension:

```json
{
  "type": "Note",
  "to": ["https://friend.example/users/alice"],
  "content": "Encrypted dais message. Open in a dais client.",
  "daisEncryptedMessage": {
    "v": 2,
    "protocol": "mls-rfc9420",
    "groupId": "base64url",
    "epoch": 3,
    "senderDeviceId": "primary",
    "ciphertext": "base64url MLS private message"
  }
}
```

The existing `encryptedMessage` v1 envelope remains valid for hosted fallback
posts. MLS DMs should use a new `daisEncryptedMessage` field so the compatibility
line is clear.

## Prototype Plan

1. Add `core/src/e2ee_mls/` with OpenMLS dependency, feature-gated if needed for
   wasm build size.
2. Add D1 tables:
   - `e2ee_devices`
   - `e2ee_peer_devices`
   - `e2ee_conversations`
   - `e2ee_messages`
3. Publish the local device KeyPackage in the actor document.
4. Add CLI commands:
   - `dais e2ee device init`
   - `dais e2ee peer inspect <actor-url>`
   - `dais e2ee peer trust <actor-url> <fingerprint>`
   - `dais dm send <actor-url> <text>`
   - `dais dm read`
5. Send one 1:1 MLS encrypted ActivityPub direct message between two dais nodes.
6. Ingest the message, reject untrusted key changes, decrypt locally, and display
   it in CLI/TUI.
7. Add lifecycle tests with two local D1 instances before live federation tests.

## Deferred

- Large groups.
- Multi-device history sync and recovery.
- Metadata privacy.
- Post-quantum MLS ciphersuites.
- Interop with non-dais clients.
- Migration of existing v1 hosted fallback posts into MLS groups.

## Decision

Proceed with MLS/OpenMLS over signed ActivityPub delivery as the dais E2EE DM
backbone. Keep AP DMs and Bluesky chat out of the security model; they are
transport/fallback surfaces, not confidentiality boundaries.
