"""Pytest configuration and shared fixtures."""

import pytest
from pathlib import Path
import tempfile
import shutil
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.backends import default_backend

from dais_cli.config import Config


@pytest.fixture
def temp_dir():
    """Create a temporary directory for tests."""
    temp_path = Path(tempfile.mkdtemp())
    yield temp_path
    shutil.rmtree(temp_path)


@pytest.fixture
def test_config(temp_dir):
    """Create a test Config instance with temporary directory."""
    return Config(config_dir=temp_dir)


@pytest.fixture
def test_keys():
    """Generate test RSA keypair."""
    private_key = rsa.generate_private_key(
        public_exponent=65537,
        key_size=2048,  # Smaller for faster tests
        backend=default_backend()
    )

    private_pem = private_key.private_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PrivateFormat.PKCS8,
        encryption_algorithm=serialization.NoEncryption()
    ).decode('utf-8')

    public_key = private_key.public_key()
    public_pem = public_key.public_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PublicFormat.SubjectPublicKeyInfo
    ).decode('utf-8')

    return {
        'private_key': private_key,
        'private_pem': private_pem,
        'public_key': public_key,
        'public_pem': public_pem
    }


@pytest.fixture
def test_config_with_keys(test_config, test_keys):
    """Create a test Config with generated keys saved."""
    test_config.config_dir.mkdir(parents=True, exist_ok=True)
    test_config.keys_dir.mkdir(parents=True, exist_ok=True)

    # Save keys
    private_key_path = test_config.keys_dir / "private.pem"
    public_key_path = test_config.keys_dir / "public.pem"

    with open(private_key_path, 'w') as f:
        f.write(test_keys['private_pem'])

    with open(public_key_path, 'w') as f:
        f.write(test_keys['public_pem'])

    # Save config
    config_data = {
        "server": {
            "domain": "test.example.com",
            "activitypub_domain": "social.test.example.com",
            "username": "testuser",
        },
        "cloudflare": {
            "account_id": "test-account-id",
            "api_token": "test-token",
            "d1_database_id": "test-db-id",
            "r2_bucket": "test-media",
        },
        "keys": {
            "private_key_path": str(private_key_path),
            "public_key_path": str(public_key_path),
        }
    }
    test_config.save(config_data)

    return test_config


@pytest.fixture
def fixture_keys_path():
    """Path to fixture test keys."""
    return Path(__file__).parent / "fixtures"
