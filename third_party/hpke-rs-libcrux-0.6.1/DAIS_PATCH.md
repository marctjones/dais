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

Remove this vendored patch when upstream publishes a compatible release that
depends on `libcrux-aead 0.0.8` or newer.
