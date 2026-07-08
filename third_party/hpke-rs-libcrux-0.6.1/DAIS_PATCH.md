# Dais Patch

This is `hpke-rs-libcrux` 0.6.1 vendored from crates.io with one dependency
change:

- `libcrux-aead` is bumped from `0.0.7` to `0.0.8`.

Reason: GitHub Dependabot reports `libcrux-chacha20poly1305 < 0.0.8` as a high
severity advisory. `hpke-rs-libcrux` 0.6.1 pins `libcrux-aead 0.0.7`, which in
turn pins the vulnerable `libcrux-chacha20poly1305 0.0.7`. No newer
`hpke-rs-libcrux` or `openmls_rust_crypto` release is currently available, so
the narrow patch keeps the OpenMLS path on the shipped provider while allowing
Cargo to resolve the patched libcrux AEAD implementation.

## Exit plan

Remove this vendored patch when upstream publishes a compatible release that
depends on `libcrux-aead 0.0.8` or newer.

Before removing it:

1. Check `cargo info hpke-rs-libcrux`, `cargo info openmls_rust_crypto`, and
   the upstream changelog for a release that no longer pins the vulnerable
   `libcrux-chacha20poly1305 0.0.7` path.
2. Replace the `[patch.crates-io]` entries in `core/Cargo.toml`,
   `client/Cargo.toml`, and `apps/dais-desk/Cargo.toml` with the upstream crate
   version.
3. Regenerate `client/Cargo.lock` and `apps/dais-desk/Cargo.lock`, then confirm
   every lockfile resolves `libcrux-chacha20poly1305 >= 0.0.8`.
4. Run `scripts/audit-e2ee-mls-security.sh` and the full server release gate
   before tagging the dependency cleanup.
