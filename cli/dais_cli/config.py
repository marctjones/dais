"""Configuration management for dais CLI."""

import os
from pathlib import Path
from typing import Optional
import toml


class Config:
    """Configuration manager for dais CLI."""

    def __init__(self, config_dir: Optional[Path] = None):
        """Initialize configuration.

        Args:
            config_dir: Override default config directory (~/.dais)
        """
        self.config_dir = config_dir or Path.home() / ".dais"
        self.config_file = self.config_dir / "config.toml"
        self.keys_dir = self.config_dir / "keys"

        self._config = None

    def load(self) -> dict:
        """Load configuration from file.

        Returns:
            Configuration dictionary
        """
        if not self.config_file.exists():
            return self._default_config()

        self._config = toml.load(self.config_file)
        return self._config

    def save(self, config: dict):
        """Save configuration to file.

        Args:
            config: Configuration dictionary to save
        """
        self.config_dir.mkdir(parents=True, exist_ok=True)
        with open(self.config_file, "w") as f:
            toml.dump(config, f)
        self._config = config

    def _default_config(self) -> dict:
        """Get default configuration template (values set during init)."""
        return {
            "server": {
                "domain": "example.com",
                "activitypub_domain": "social.example.com",
                "username": "social",
            },
            "cloudflare": {
                "account_id": "",
                "api_token": "",
                "d1_database_id": "",
                "r2_bucket": "dais-media",
            },
            "keys": {
                "private_key_path": str(self.keys_dir / "private.pem"),
                "public_key_path": str(self.keys_dir / "public.pem"),
            }
        }

    def get(self, key: str, default=None):
        """Get a configuration value.

        Args:
            key: Dot-separated configuration key (e.g., "server.domain")
            default: Default value if key not found

        Returns:
            Configuration value or default
        """
        if self._config is None:
            self.load()

        keys = key.split(".")
        value = self._config

        for k in keys:
            if isinstance(value, dict) and k in value:
                value = value[k]
            else:
                return default

        return value

    def set(self, key: str, value):
        """Set a configuration value.

        Args:
            key: Dot-separated configuration key (e.g., "server.domain")
            value: Value to set
        """
        if self._config is None:
            self.load()

        keys = key.split(".")
        config = self._config

        for k in keys[:-1]:
            if k not in config:
                config[k] = {}
            config = config[k]

        config[keys[-1]] = value
        self.save(self._config)
