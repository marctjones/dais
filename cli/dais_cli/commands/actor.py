"""Actor management commands."""

import click
from rich.console import Console
from pathlib import Path
import subprocess
import sys
import uuid

from dais_cli.config import Config

console = Console()


@click.group()
def actor():
    """Manage actors."""
    pass


@actor.command()
@click.option('--remote', is_flag=True, help='Seed to remote database')
def seed(remote):
    """Seed the initial actor into the database.

    Reads keys from ~/.dais/keys/ and inserts the actor record.
    """
    config = Config()
    config.load()

    username = config.get("server.username", "marc")
    domain = config.get("server.activitypub_domain", "social.dais.social")

    # Read keys
    private_key_path = Path(config.get("keys.private_key_path"))
    public_key_path = Path(config.get("keys.public_key_path"))

    if not private_key_path.exists():
        console.print("[red]Private key not found. Run 'dais setup init' first.[/red]")
        sys.exit(1)

    if not public_key_path.exists():
        console.print("[red]Public key not found. Run 'dais setup init' first.[/red]")
        sys.exit(1)

    console.print("[bold blue]Seeding actor into database...[/bold blue]\n")

    # Read keys
    with open(private_key_path, "r") as f:
        private_key = f.read()

    with open(public_key_path, "r") as f:
        public_key = f.read()

    # Generate actor ID
    actor_id = f"https://{domain}/users/{username}"
    inbox_url = f"{actor_id}/inbox"
    outbox_url = f"{actor_id}/outbox"
    followers_url = f"{actor_id}/followers"
    following_url = f"{actor_id}/following"

    # Escape single quotes in keys for SQL
    private_key_escaped = private_key.replace("'", "''")
    public_key_escaped = public_key.replace("'", "''")

    # Create SQL
    sql = f"""
    INSERT OR REPLACE INTO actors (
        id, username, display_name, summary, public_key, private_key,
        inbox_url, outbox_url, followers_url, following_url
    ) VALUES (
        '{actor_id}',
        '{username}',
        'Marc',
        'Building my own corner of the fediverse',
        '{public_key_escaped}',
        '{private_key_escaped}',
        '{inbox_url}',
        '{outbox_url}',
        '{followers_url}',
        '{following_url}'
    );
    """

    cmd = ["wrangler", "d1", "execute", "dais-social", "--command", sql]
    if remote:
        cmd.append("--remote")

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        console.print(f"[green]✓[/green] Actor '{username}' seeded successfully")
        console.print(f"[dim]Actor ID: {actor_id}[/dim]")

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Failed to seed actor[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)


@actor.command()
@click.option('--remote', is_flag=True, help='Query remote database')
def show(remote):
    """Show current actor information."""
    cmd = [
        "wrangler", "d1", "execute", "dais-social",
        "--command", "SELECT id, username, display_name, summary FROM actors LIMIT 1"
    ]
    if remote:
        cmd.append("--remote")

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        console.print("[bold blue]Actor Information[/bold blue]\n")
        console.print(result.stdout)

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Error querying actor[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)
