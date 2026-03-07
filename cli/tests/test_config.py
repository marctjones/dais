"""Tests for configuration management."""

import pytest
from dais_cli.config import Config


def test_config_default_path(temp_dir):
    """Test default config path."""
    config = Config()
    assert config.config_dir.name == ".dais"
    assert config.config_file.name == "config.toml"


def test_config_custom_path(temp_dir):
    """Test custom config directory."""
    config = Config(config_dir=temp_dir)
    assert config.config_dir == temp_dir


def test_config_save_and_load(test_config):
    """Test saving and loading configuration."""
    config_data = {
        "server": {
            "domain": "example.com",
            "username": "testuser"
        }
    }

    test_config.save(config_data)
    assert test_config.config_file.exists()

    loaded = test_config.load()
    assert loaded["server"]["domain"] == "example.com"
    assert loaded["server"]["username"] == "testuser"


def test_config_get(test_config):
    """Test getting configuration values."""
    config_data = {
        "server": {
            "domain": "example.com",
            "username": "testuser"
        }
    }
    test_config.save(config_data)

    assert test_config.get("server.domain") == "example.com"
    assert test_config.get("server.username") == "testuser"
    assert test_config.get("nonexistent.key") is None
    assert test_config.get("nonexistent.key", "default") == "default"


def test_config_set(test_config):
    """Test setting configuration values."""
    test_config.load()  # Load default or empty config
    test_config.set("server.domain", "newdomain.com")

    loaded = test_config.load()
    assert loaded["server"]["domain"] == "newdomain.com"


def test_config_nested_set(test_config):
    """Test setting nested configuration values."""
    test_config.load()
    test_config.set("new.nested.key", "value")

    loaded = test_config.load()
    assert loaded["new"]["nested"]["key"] == "value"


def test_config_load_nonexistent_returns_default(test_config):
    """Test loading nonexistent config returns defaults."""
    loaded = test_config.load()
    assert "server" in loaded
    assert "cloudflare" in loaded
    assert "keys" in loaded
