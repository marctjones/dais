"""Media management commands."""

import click
from rich.console import Console
from rich.table import Table
import subprocess
import sys
from pathlib import Path

console = Console()


@click.group()
def media():
    """Manage media files in R2 storage."""
    pass


@media.command()
@click.option('--remote', is_flag=True, help='Query remote R2 bucket')
def list(remote):
    """List all media files in R2 storage.

    Note: Wrangler CLI doesn't support listing objects directly.
    Use the Cloudflare dashboard to browse R2 bucket contents.
    """
    console.print("[bold yellow]Media File Listing[/bold yellow]\n")
    console.print("[dim]Wrangler CLI doesn't support listing R2 objects.[/dim]")
    console.print("[dim]To view your media files:[/dim]\n")
    console.print("  1. Go to https://dash.cloudflare.com")
    console.print("  2. Navigate to R2 → dais-media bucket")
    console.print("  3. Browse files in the dashboard\n")
    console.print("[dim]Alternatively, media URLs are stored in your posts:[/dim]")
    console.print("  dais post list --remote")


@media.command()
def stats():
    """Show R2 storage statistics.

    Note: Storage metrics require Cloudflare dashboard or API access.
    """
    console.print("[bold yellow]R2 Storage Statistics[/bold yellow]\n")
    console.print("[dim]Storage metrics are available in the Cloudflare dashboard:[/dim]\n")
    console.print("  1. Go to https://dash.cloudflare.com")
    console.print("  2. Navigate to R2 → dais-media bucket")
    console.print("  3. View 'Metrics' tab for storage usage and request counts\n")
    console.print("[dim]Or check post media usage:[/dim]")
    console.print("  dais post list --remote | grep 'Total'")


@media.command()
@click.argument('filename')
@click.option('--remote', is_flag=True, help='Delete from remote R2 bucket')
@click.confirmation_option(prompt='Are you sure you want to delete this media file?')
def delete(filename, remote):
    """Delete a media file from R2 storage.

    FILENAME: Name of the file to delete (e.g., "20260309120000-abc123.jpg")
    """
    console.print(f"[bold blue]Deleting {filename}...[/bold blue]\n")

    cmd = ["wrangler", "r2", "object", "delete", f"dais-media/{filename}"]
    if remote:
        cmd.append("--remote")

    try:
        subprocess.run(cmd, capture_output=True, text=True, check=True)
        console.print(f"[green]✓[/green] Deleted {filename}")

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Error deleting file[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)


@media.command()
def cleanup():
    """Find and remove orphaned media files.

    Note: Automatic cleanup requires R2 API access not available via wrangler CLI.
    """
    console.print("[bold yellow]Media Cleanup[/bold yellow]\n")
    console.print("[dim]Automatic cleanup is not yet implemented.[/dim]\n")
    console.print("[dim]To manually clean up orphaned media:[/dim]\n")
    console.print("  1. List files in dashboard: https://dash.cloudflare.com → R2 → dais-media")
    console.print("  2. Check if file is referenced: dais post list --remote")
    console.print("  3. Delete manually if orphaned: dais media delete <filename> --remote\n")
    console.print("[dim]Future: This will automatically detect and remove orphaned files[/dim]")
