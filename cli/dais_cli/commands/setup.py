"""Setup and initialization commands."""

import click
import json
from rich.console import Console
from rich.panel import Panel
from pathlib import Path
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.backends import default_backend

from dais_cli.config import Config, get_dais_dir

console = Console()


@click.group()
def setup():
    """Initialize and configure dais.social server."""
    pass


@setup.command()
@click.option('--force', is_flag=True, help='Overwrite existing configuration')
def init(force):
    """Initialize configuration and generate RSA keys.

    This command:
    - Creates ~/.dais/ directory
    - Generates RSA key pair for HTTP signatures
    - Creates default configuration file
    """
    config = Config()

    # Check if already initialized
    if config.config_file.exists() and not force:
        console.print(
            "[yellow]Configuration already exists. Use --force to overwrite.[/yellow]"
        )
        return

    # Create directories
    config.config_dir.mkdir(parents=True, exist_ok=True)
    config.keys_dir.mkdir(parents=True, exist_ok=True)

    console.print("[bold blue]Initializing dais configuration...[/bold blue]\n")

    # Prompt for configuration
    console.print("[bold]Server Configuration:[/bold]\n")

    username = click.prompt(
        "Username (e.g., 'social' for business, your name for personal)",
        default="social",
        type=str
    )

    domain = click.prompt(
        "Your domain (e.g., 'yourdomain.com')",
        default="example.com",
        type=str
    )

    activitypub_domain = click.prompt(
        "ActivityPub subdomain (where workers will run)",
        default=f"social.{domain}",
        type=str
    )

    console.print(f"\n[dim]Your Fediverse identity will be: @{username}@{domain}[/dim]\n")

    # Generate RSA key pair
    console.print("Generating RSA key pair (4096 bits)...")
    private_key = rsa.generate_private_key(
        public_exponent=65537,
        key_size=4096,
        backend=default_backend()
    )

    public_key = private_key.public_key()

    # Save private key
    private_key_path = config.keys_dir / "private.pem"
    with open(private_key_path, "wb") as f:
        f.write(
            private_key.private_bytes(
                encoding=serialization.Encoding.PEM,
                format=serialization.PrivateFormat.PKCS8,
                encryption_algorithm=serialization.NoEncryption()
            )
        )
    private_key_path.chmod(0o600)
    console.print(f"  [green]✓[/green] Private key saved to {private_key_path}")

    # Save public key
    public_key_path = config.keys_dir / "public.pem"
    with open(public_key_path, "wb") as f:
        f.write(
            public_key.public_bytes(
                encoding=serialization.Encoding.PEM,
                format=serialization.PublicFormat.SubjectPublicKeyInfo
            )
        )
    console.print(f"  [green]✓[/green] Public key saved to {public_key_path}")

    # Create configuration with user values
    default_config = config._default_config()
    default_config['server']['username'] = username
    default_config['server']['domain'] = domain
    default_config['server']['activitypub_domain'] = activitypub_domain
    config.save(default_config)
    console.print(f"  [green]✓[/green] Configuration saved to {config.config_file}")

    console.print("\n[bold green]Initialization complete![/bold green]\n")

    console.print(Panel(
        f"""[bold]Next steps:[/bold]

1. Update Cloudflare credentials in {config.config_file}
2. Deploy the WebFinger Worker: cd workers/webfinger && wrangler deploy
3. Test WebFinger: dais test webfinger
4. Create your first post: dais post create "Hello, Fediverse!"
        """,
        title="Setup Complete",
        border_style="green"
    ))


@setup.command()
def show():
    """Show current configuration."""
    config = Config()

    if not config.config_file.exists():
        console.print("[yellow]Configuration not found. Run 'dais setup init' first.[/yellow]")
        return

    config_data = config.load()

    console.print(Panel(
        f"""[bold]Server Configuration:[/bold]
  Domain: {config_data['server']['domain']}
  ActivityPub Domain: {config_data['server']['activitypub_domain']}
  Username: {config_data['server']['username']}

[bold]Cloudflare:[/bold]
  Account ID: {config_data['cloudflare']['account_id'] or '[dim]not set[/dim]'}
  D1 Database ID: {config_data['cloudflare']['d1_database_id'] or '[dim]not set[/dim]'}
  R2 Bucket: {config_data['cloudflare']['r2_bucket']}

[bold]Keys:[/bold]
  Private Key: {config_data['keys']['private_key_path']}
  Public Key: {config_data['keys']['public_key_path']}
        """,
        title="dais.social Configuration",
        border_style="blue"
    ))


@setup.command()
@click.option('--force', is_flag=True, help='Overwrite existing Bluesky configuration')
def bluesky(force):
    """Configure Bluesky (AT Protocol) credentials.

    This allows dais to post to Bluesky alongside ActivityPub.
    You'll need your Bluesky handle and password (or app password).

    Example:
        dais setup bluesky
    """
    bluesky_config_path = get_dais_dir() / "bluesky.json"

    if bluesky_config_path.exists() and not force:
        console.print("[yellow]Bluesky already configured. Use --force to overwrite.[/yellow]")
        return

    console.print("[bold blue]Bluesky (AT Protocol) Setup[/bold blue]\n")

    console.print("Enter your Bluesky credentials:")
    console.print("[dim]Get an app password at: https://bsky.app/settings/app-passwords[/dim]\n")

    handle = click.prompt(
        "Bluesky handle (e.g., username.bsky.social)",
        type=str
    )

    password = click.prompt(
        "Bluesky password or app password",
        hide_input=True,
        type=str
    )

    # Test the credentials
    console.print("\n[dim]Testing credentials...[/dim]")
    try:
        from atproto import Client
        client = Client()
        session = client.login(handle, password)

        console.print(f"[green]✓[/green] Successfully authenticated as {session.handle}")

        # Save credentials
        bluesky_config = {
            "handle": handle,
            "password": password,
            "did": session.did
        }

        bluesky_config_path.parent.mkdir(parents=True, exist_ok=True)
        with open(bluesky_config_path, 'w') as f:
            json.dump(bluesky_config, f, indent=2)

        bluesky_config_path.chmod(0o600)
        console.print(f"[green]✓[/green] Credentials saved to {bluesky_config_path}")

        console.print("\n[bold green]Bluesky setup complete![/bold green]")
        console.print("\n[dim]You can now post to both ActivityPub and Bluesky with:[/dim]")
        console.print("[dim]  dais post create \"Hello!\" --protocol both --remote[/dim]\n")

    except ImportError:
        console.print("[red]✗[/red] atproto library not installed")
        console.print("[dim]Install with: pip install atproto[/dim]")
    except Exception as e:
        console.print(f"[red]✗[/red] Authentication failed: {e}")
        console.print("[dim]Check your handle and password[/dim]")
