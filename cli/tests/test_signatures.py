"""Tests for HTTP signature generation and verification."""

import pytest
import base64
import hashlib
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import padding


def test_sign_message(test_keys):
    """Test RSA signature generation."""
    message = "test message"

    # Sign with private key
    signature_bytes = test_keys['private_key'].sign(
        message.encode('utf-8'),
        padding.PKCS1v15(),
        hashes.SHA256()
    )
    signature_b64 = base64.b64encode(signature_bytes).decode('utf-8')

    assert len(signature_b64) > 0
    assert signature_b64.endswith('=') or signature_b64.endswith('==') or len(signature_b64) % 4 == 0


def test_signature_verification(test_keys):
    """Test signature verification with correct key."""
    message = "test message"

    # Sign
    signature_bytes = test_keys['private_key'].sign(
        message.encode('utf-8'),
        padding.PKCS1v15(),
        hashes.SHA256()
    )

    # Verify
    try:
        test_keys['public_key'].verify(
            signature_bytes,
            message.encode('utf-8'),
            padding.PKCS1v15(),
            hashes.SHA256()
        )
        verified = True
    except Exception:
        verified = False

    assert verified


def test_signature_fails_with_wrong_message(test_keys):
    """Test that signature verification fails with wrong message."""
    message = "test message"
    wrong_message = "wrong message"

    # Sign original message
    signature_bytes = test_keys['private_key'].sign(
        message.encode('utf-8'),
        padding.PKCS1v15(),
        hashes.SHA256()
    )

    # Try to verify with wrong message
    with pytest.raises(Exception):
        test_keys['public_key'].verify(
            signature_bytes,
            wrong_message.encode('utf-8'),
            padding.PKCS1v15(),
            hashes.SHA256()
        )


def test_digest_calculation():
    """Test SHA-256 digest calculation for request body."""
    body = '{"type":"Follow","actor":"https://example.com/users/alice"}'

    body_hash = hashlib.sha256(body.encode('utf-8')).digest()
    digest = 'SHA-256=' + base64.b64encode(body_hash).decode('utf-8')

    # Verify format
    assert digest.startswith('SHA-256=')
    assert len(digest) > len('SHA-256=')

    # Verify it's base64
    digest_b64 = digest.replace('SHA-256=', '')
    decoded = base64.b64decode(digest_b64)
    assert len(decoded) == 32  # SHA-256 is 32 bytes


def test_signing_string_construction():
    """Test building the signing string from request components."""
    method = "POST"
    path = "/users/testuser/inbox"
    host = "social.test.example.com"
    date = "Mon, 01 Jan 2024 00:00:00 GMT"
    digest = "SHA-256=abcdef123456"

    signing_string = f"(request-target): {method.lower()} {path}\nhost: {host}\ndate: {date}\ndigest: {digest}"

    expected_parts = [
        "(request-target): post /users/testuser/inbox",
        "host: social.test.example.com",
        "date: Mon, 01 Jan 2024 00:00:00 GMT",
        "digest: SHA-256=abcdef123456"
    ]

    for part in expected_parts:
        assert part in signing_string

    # Verify newline separation
    assert signing_string.count('\n') == 3


def test_signature_header_format(test_keys):
    """Test HTTP Signature header format."""
    key_id = "https://social.test.example.com/users/testuser#main-key"
    algorithm = "rsa-sha256"
    headers = "(request-target) host date digest"

    message = "(request-target): post /users/testuser/inbox\nhost: social.test.example.com\ndate: Mon, 01 Jan 2024 00:00:00 GMT\ndigest: SHA-256=abcdef"

    signature_bytes = test_keys['private_key'].sign(
        message.encode('utf-8'),
        padding.PKCS1v15(),
        hashes.SHA256()
    )
    signature_b64 = base64.b64encode(signature_bytes).decode('utf-8')

    signature_header = (
        f'keyId="{key_id}",'
        f'algorithm="{algorithm}",'
        f'headers="{headers}",'
        f'signature="{signature_b64}"'
    )

    assert 'keyId=' in signature_header
    assert 'algorithm="rsa-sha256"' in signature_header
    assert 'headers="(request-target) host date digest"' in signature_header
    assert 'signature=' in signature_header
