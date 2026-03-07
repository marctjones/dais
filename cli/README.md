# dais CLI

Command-line tools for managing your dais.social ActivityPub server.

## Installation

```bash
# Install in development mode
pip install -e .

# Or install from wheel
pip install dais-cli
```

## Usage

```bash
# Show version and help
dais --version
dais --help

# Initialize configuration and generate keys
dais setup init

# Create a post
dais post create "Hello, Fediverse!"
dais post create "Hello world" --visibility unlisted

# List posts
dais post list
dais post list --limit 10

# Manage followers
dais followers list
dais followers approve @alice@mastodon.social
dais followers reject @bob@pleroma.example

# Test federation
dais test webfinger
dais test federation @someone@instance.social

# Show statistics
dais stats
```

## Commands

### Setup

- `dais setup init` - Initialize configuration, generate RSA keys
- `dais setup show` - Show current configuration

### Posts

- `dais post create <content>` - Create and publish a post
- `dais post list` - List your posts
- `dais post delete <id>` - Delete a post

### Followers

- `dais followers list` - List all followers
- `dais followers approve <actor>` - Approve a follow request
- `dais followers reject <actor>` - Reject a follow request
- `dais followers remove <actor>` - Remove a follower

### Testing

- `dais test webfinger` - Test WebFinger endpoint
- `dais test actor` - Test Actor endpoint
- `dais test federation <actor>` - Test federation with another instance

### Statistics

- `dais stats` - Show follower count, post count, etc.

## Configuration

The CLI uses a `.dais` directory in your home folder for configuration:

```
~/.dais/
  config.toml       # Configuration file
  keys/
    private.pem     # RSA private key
    public.pem      # RSA public key
```

## Development

```bash
# Install with dev dependencies
pip install -e ".[dev]"

# Run tests
pytest

# Format code
black .
ruff check .
```
