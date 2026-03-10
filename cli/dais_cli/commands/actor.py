"""Actor management commands."""

import click
from rich.console import Console
from pathlib import Path
import subprocess
import sys
import uuid

from dais_cli.config import Config
from dais_cli.media import upload_to_r2, generate_media_url

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


@actor.command()
@click.option('--name', type=str, help='Update display name')
@click.option('--bio', type=str, help='Update bio/summary')
@click.option('--avatar', type=click.Path(exists=True), help='Upload avatar image')
@click.option('--header', type=click.Path(exists=True), help='Upload header/banner image')
@click.option('--remote', is_flag=True, help='Update remote database')
def update(name, bio, avatar, header, remote):
    """Update actor profile information.

    Examples:
        dais actor update --name "Marc Jonathan" --remote
        dais actor update --bio "Decentralized everything!" --remote
        dais actor update --avatar avatar.jpg --remote
        dais actor update --header banner.jpg --remote
    """
    if not any([name, bio, avatar, header]):
        console.print("[yellow]No updates specified. Use --name, --bio, --avatar, or --header[/yellow]")
        sys.exit(0)

    console.print("[bold blue]Updating actor profile...[/bold blue]\n")

    config = Config()
    updates = []
    media_domain = config.get("cloudflare.r2_public_domain", "social.dais.social")

    # Handle text updates
    if name:
        console.print(f"[dim]Setting display name: {name}[/dim]")
        name_escaped = name.replace("'", "''")
        updates.append(f"display_name = '{name_escaped}'")

    if bio:
        console.print(f"[dim]Setting bio: {bio[:50]}{'...' if len(bio) > 50 else ''}[/dim]")
        bio_escaped = bio.replace("'", "''")
        updates.append(f"summary = '{bio_escaped}'")
    # Handle avatar upload
    if avatar:
        console.print(f"[dim]Uploading avatar...[/dim]")
        filename = upload_to_r2(avatar, remote=remote)
        if filename:
            avatar_url = generate_media_url(filename, media_domain)
            console.print(f"[green]✓[/green] Avatar uploaded: {avatar_url}")
            updates.append(f"icon = '{avatar_url}'")
        else:
            console.print("[red]✗ Avatar upload failed[/red]")
            sys.exit(1)

    # Handle header upload
    if header:
        console.print(f"[dim]Uploading header...[/dim]")
        filename = upload_to_r2(header, remote=remote)
        if filename:
            header_url = generate_media_url(filename, media_domain)
            console.print(f"[green]✓[/green] Header uploaded: {header_url}")
            updates.append(f"image = '{header_url}'")
        else:
            console.print("[red]✗ Header upload failed[/red]")
            sys.exit(1)

    # Build and execute UPDATE query
    if not updates:
        console.print("[yellow]No valid updates to apply[/yellow]")
        sys.exit(0)

    update_clause = ", ".join(updates)
    sql = f"UPDATE actors SET {update_clause} WHERE 1=1"

    # Get project root for wrangler command
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    cmd = ["wrangler", "d1", "execute", "DB", "--command", sql]
    if remote:
        cmd.append("--remote")

    try:
        subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        console.print(f"\n[green]✓[/green] Profile updated successfully")

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Failed to update profile[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)
