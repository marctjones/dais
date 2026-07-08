# E2EE and MLS Security Review

The post-roadmap security gate is:

```bash
scripts/audit-e2ee-mls-security.sh
```

The script writes a report under `tmp/e2ee-mls-security-*/` and checks:

- production E2EE/MLS paths do not contain unclassified fake or test-only
  behavior;
- client and Dais Desk lockfiles resolve `libcrux-chacha20poly1305` to the
  patched `0.0.8` version;
- the vendored `hpke-rs-libcrux` patch includes an exit plan;
- core MLS tests cover successful decrypt, state restore, stale epoch rejection,
  malformed ciphertext rejection, wrong protocol rejection, removed-device
  decrypt failure, and multi-device/group restart behavior;
- client-core and Desk E2EE tests cover v1 decrypt success, wrong key failure,
  invalid envelope rejection, decrypted Desk rendering, and specific decrypt
  failure messages.

## Review Notes

- MLS/OpenMLS remains the Dais v2 path for direct and group encrypted messages.
  The server stores delivery and metadata needed for routing, not plaintext.
- Decrypt failure is an error state for local clients. UI surfaces must show a
  specific reason, such as missing local private key, wrong/corrupt payload, or
  missing MLS group state.
- Test fixtures may generate encrypted messages, but production code must never
  replace decrypt with canned plaintext.
- The `third_party/hpke-rs-libcrux-0.6.1` patch is a narrow dependency advisory
  mitigation. Remove it once upstream crates support a non-vulnerable libcrux
  chain; follow the exit plan in `third_party/hpke-rs-libcrux-0.6.1/DAIS_PATCH.md`.
