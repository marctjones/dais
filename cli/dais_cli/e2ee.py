"""End-to-end encryption for dais messages, with graceful fallback for clients
that don't support it.

WIRE FORMAT (the important part for fediverse interop):
The ActivityPub Note's `content` field carries a human-readable FALLBACK NOTICE,
so non-supporting clients (Mastodon, etc.) show the recipient that they received
an encrypted message and how to read it — instead of silent gibberish. The actual
ciphertext travels in an `encryptedMessage` extension property that dais clients
understand and non-supporting clients harmlessly ignore.

CRYPTO (v1): hybrid AES-256-GCM content encryption + RSA-OAEP(SHA-256) key
wrapping to each recipient's published RSA public key. This is a pragmatic first
cut. Roadmap (#71): move to MLS (RFC 9420) for forward secrecy, post-compromise
security, and efficient groups, with dedicated encryption keys separate from the
HTTP-signature key. (v1 reuses the actor RSA key — fine for a first cut, flagged
for replacement.)
"""
import os
import base64
from typing import Dict

from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.asymmetric import padding
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.backends import default_backend


def _b64(b: bytes) -> str:
    return base64.b64encode(b).decode("ascii")


def _unb64(s: str) -> bytes:
    return base64.b64decode(s)


def encrypt_message(plaintext: str, recipients: Dict[str, str]) -> dict:
    """Encrypt `plaintext` for one or more recipients.

    Args:
        recipients: {key_id: public_key_pem} — one entry per recipient.

    Returns the `encryptedMessage` extension structure (JSON-serializable).
    """
    if not recipients:
        raise ValueError("at least one recipient public key is required")

    # 1. Encrypt the content once with a fresh symmetric key.
    cek = AESGCM.generate_key(bit_length=256)
    nonce = os.urandom(12)
    ciphertext = AESGCM(cek).encrypt(nonce, plaintext.encode("utf-8"), None)

    # 2. Wrap the content key to each recipient's public key.
    wrapped = []
    for key_id, pem in recipients.items():
        pub = serialization.load_pem_public_key(pem.encode("utf-8"), backend=default_backend())
        wk = pub.encrypt(
            cek,
            padding.OAEP(mgf=padding.MGF1(hashes.SHA256()), algorithm=hashes.SHA256(), label=None),
        )
        wrapped.append({"keyId": key_id, "wrappedKey": _b64(wk)})

    return {
        "v": 1,
        "alg": "AES-256-GCM",
        "keyWrap": "RSA-OAEP-256",
        "iv": _b64(nonce),
        "ciphertext": _b64(ciphertext),
        "recipients": wrapped,
    }


def decrypt_message(enc: dict, private_key_pem: str, my_key_id: str = None) -> str:
    """Decrypt an `encryptedMessage` with our private key."""
    recips = enc.get("recipients", [])
    mine = None
    if my_key_id:
        mine = next((r for r in recips if r.get("keyId") == my_key_id), None)
    if mine is None and len(recips) == 1:
        mine = recips[0]
    if mine is None:
        raise ValueError("no matching recipient key for this message")

    priv = serialization.load_pem_private_key(
        private_key_pem.encode("utf-8"), password=None, backend=default_backend()
    )
    cek = priv.decrypt(
        _unb64(mine["wrappedKey"]),
        padding.OAEP(mgf=padding.MGF1(hashes.SHA256()), algorithm=hashes.SHA256(), label=None),
    )
    return AESGCM(cek).decrypt(_unb64(enc["iv"]), _unb64(enc["ciphertext"]), None).decode("utf-8")


def fallback_content(view_url: str = None) -> str:
    """The notice rendered by clients that can't decrypt (e.g. Mastodon).

    This is what makes E2EE graceful over the fediverse: instead of gibberish,
    the recipient sees that they got an encrypted message and how to read it.
    """
    link = ""
    if view_url:
        link = f'To read it, open it in dais: <a href="{view_url}">{view_url}</a><br>'
    return (
        "🔒 <strong>End-to-end encrypted message</strong><br>"
        "This message was sent encrypted, so your current client can’t display it.<br>"
        f"{link}"
        '<em>You’ll need a dais-compatible client to read it — learn more at '
        '<a href="https://dais.social">dais.social</a>.</em>'
    )
