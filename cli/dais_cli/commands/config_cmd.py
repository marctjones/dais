"""Configuration management commands."""

import click
from rich.console import Console
from rich.table import Table

from dais_cli.config import Config

console = Console()


@click.group(name='config')
def config():
    """Manage dais configuration."""
    pass


@config.command()
@click.argument('key')
@click.argument('value')
def set(key, value):
    """Set a configuration value.

    Examples:
        dais config set server.username social
        dais config set server.domain yourdomain.com
        dais config set cloudflare.account_id abc123
    """
    cfg = Config()

    if not cfg.config_file.exists():
        console.print("[yellow]Configuration not found. Run 'dais setup init' first.[/yellow]")
        return

    cfg.load()
    cfg.set(key, value)
    console.print(f"[green]✓[/green] Set {key} = {value}")


@config.command()
@click.argument('key')
def get(key):
    """Get a configuration value.

    Examples:
        dais config get server.username
        dais config get server.domain
    """
    cfg = Config()

    if not cfg.config_file.exists():
        console.print("[yellow]Configuration not found. Run 'dais setup init' first.[/yellow]")
        return

    value = cfg.get(key)

    if value is None:
        console.print(f"[yellow]Key '{key}' not found[/yellow]")
    else:
        console.print(f"{key} = {value}")


@config.command(name='list')
def list_config():
    """List all configuration values."""
    cfg = Config()

    if not cfg.config_file.exists():
        console.print("[yellow]Configuration not found. Run 'dais setup init' first.[/yellow]")
        return

    config_data = cfg.load()

    table = Table(title="dais Configuration", show_header=True, header_style="bold blue")
    table.add_column("Key", style="cyan")
    table.add_column("Value", style="green")

    def flatten_dict(d, prefix=''):
        """Flatten nested dict for display."""
        for key, value in d.items():
            full_key = f"{prefix}.{key}" if prefix else key
            if isinstance(value, dict):
                flatten_dict(value, full_key)
            else:
                table.add_row(full_key, str(value))

    flatten_dict(config_data)
    console.print(table)


@config.command()
def validate():
    """Validate configuration."""
    cfg = Config()

    if not cfg.config_file.exists():
        console.print("[yellow]Configuration not found. Run 'dais setup init' first.[/yellow]")
        return

    config_data = cfg.load()
    errors = []
    warnings = []

    # Check required fields
    if not config_data.get('server', {}).get('username'):
        errors.append("server.username is required")

    if not config_data.get('server', {}).get('domain'):
        errors.append("server.domain is required")

    if not config_data.get('server', {}).get('activitypub_domain'):
        errors.append("server.activitypub_domain is required")

    # Check optional but recommended fields
    if not config_data.get('cloudflare', {}).get('account_id'):
        warnings.append("cloudflare.account_id not set (required for deployment)")

    if not config_data.get('cloudflare', {}).get('d1_database_id'):
        warnings.append("cloudflare.d1_database_id not set (required for database operations)")

    # Check keys exist
    private_key_path = cfg.config_dir / "keys" / "private.pem"
    public_key_path = cfg.config_dir / "keys" / "public.pem"

    if not private_key_path.exists():
        errors.append(f"Private key not found at {private_key_path}")

    if not public_key_path.exists():
        errors.append(f"Public key not found at {public_key_path}")

    # Display results
    if errors:
        console.print("\n[bold red]❌ Errors:[/bold red]")
        for error in errors:
            console.print(f"  • {error}")

    if warnings:
        console.print("\n[bold yellow]⚠ Warnings:[/bold yellow]")
        for warning in warnings:
            console.print(f"  • {warning}")

    if not errors and not warnings:
        console.print("\n[bold green]✓ Configuration is valid![/bold green]")
    elif not errors:
        console.print("\n[bold green]✓ Configuration is valid (with warnings)[/bold green]")
    else:
        console.print("\n[bold red]Configuration has errors[/bold red]")
        return 1
