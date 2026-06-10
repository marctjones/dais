# Design: dais E2EE v1 Wire Format (interop contract)

**Status:** Normative spec, extracted from the validated-live Python implementation
(`cli/dais_cli/e2ee.py`, commit `dc592f1`, #71) before that client was retired in
favour of the Rust client. The Rust client (and any future client) **MUST** reproduce
this byte-for-byte to stay interoperable with messages already sent, and to keep the
graceful-fallback behaviour for non-supporting clients (Mastodon, etc.).

**Roadmap:** v1 is a pragmatic first cut. The target is MLS (RFC 9420) with dedicated
keys, forward secrecy, post-compromise security, and efficient groups (#71). This doc
describes **v1 only** — do not extend it; design v2 as MLS.

---

## 1. Threat model & guarantees (v1)

- **Confidentiality from intermediaries:** ciphertext rides any ActivityPub server;
  only holders of a recipient private key can read it. The sender's own dais server
  stores **only the fallback notice**, never plaintext.
- **NOT provided in v1:** forward secrecy, post-compromise security, deniability,
  group efficiency. v1 reuses the actor RSA key (same key as HTTP Signatures) and, in
  the shipped CLI, encrypts to self. Flagged for replacement by MLS.

---

## 2. Crypto parameters (normative)

| Field | Value |
|---|---|
| Content cipher | **AES-256-GCM**, 256-bit content-encryption key (CEK), fresh per message |
| Nonce / IV | **12 bytes**, `os.urandom` (CSPRNG); never reused with a CEK |
| AEAD associated data | **none** (`null`) |
| Key wrap | **RSA-OAEP** with **MGF1(SHA-256)** and hash **SHA-256**, label = `None` |
| Recipient key | recipient's **published RSA public key** (PEM, SPKI), keyed by ActivityPub key id |
| Encoding | all binary fields are **standard base64** (`base64.b64encode`, not URL-safe) |

The CEK is encrypted **once** with AES-GCM; the CEK itself is RSA-OAEP-wrapped
**once per recipient**. Hybrid scheme — content encrypted once, key delivered to N
recipients.

---

## 3. The `encryptedMessage` extension object

Carried as an extension property on the ActivityPub `Note` (alongside, not replacing,
`content`). dais clients read it; non-supporting clients ignore unknown properties.

```json
{
  "v": 1,
  "alg": "AES-256-GCM",
  "keyWrap": "RSA-OAEP-256",
  "iv": "<base64(12-byte nonce)>",
  "ciphertext": "<base64(AES-GCM output: ct||tag)>",
  "recipients": [
    { "keyId": "https://bob.example/actor#main-key", "wrappedKey": "<base64(RSA-OAEP(cek))>" }
  ]
}
```

- `ciphertext` is the raw AES-GCM output (ciphertext **with the 16-byte GCM tag
  appended**, as produced by the AEAD `encrypt`). Implementations using a detached tag
  must concatenate `ct || tag` to match.
- `recipients[].keyId` is the recipient's AP public-key id (`actor#main-key`).
- One `recipients` entry per recipient.

### Decryption recipient selection
1. If a `my_key_id` is known, pick the `recipients` entry whose `keyId` matches.
2. Else, if there is exactly **one** recipient, use it.
3. Else → error (ambiguous; no matching key).

Wrong-key / non-recipient decryption MUST fail (RSA-OAEP unwrap raises) rather than
return garbage.

---

## 4. The fallback `content` (what makes it graceful)

The `Note.content` delivered over the wire is **not** ciphertext — it is a
human-readable HTML notice, so Mastodon et al. render something meaningful. The dais
server stores this notice (and only this) for the sender's records.

Canonical v1 string (HTML; `{link}` present only when a view URL is known):

```
🔒 <strong>End-to-end encrypted message</strong><br>
This message was sent encrypted, so your current client can’t display it.<br>
To read it, open it in dais: <a href="{view_url}">{view_url}</a><br>
<em>You’ll need a dais-compatible client to read it — learn more at
<a href="https://dais.social">dais.social</a>.</em>
```

When no `view_url` is available, the "To read it, open it in dais: …" line is omitted.

---

## 5. Conformance vectors

The Rust port should reproduce these behaviours (verified against `e2ee.py`):

1. **Round-trip:** `decrypt(encrypt(m, {kid: pub}), priv, kid) == m`.
2. **No plaintext on the wire:** the serialized `encryptedMessage` object contains the
   plaintext nowhere.
3. **Non-recipient refusal:** decrypting with an unrelated private key raises (does not
   return plaintext).
4. **Fallback rendering:** a client with no E2EE support renders the §4 notice from
   `Note.content` (this is what a Mastodon recipient sees).

A direct port of these four checks belongs in the Rust client's test suite.
