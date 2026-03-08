"""Follower management commands."""

import click
from rich.console import Console
from rich.table import Table
import subprocess
import sys
import json
from pathlib import Path

from dais_cli.config import Config
from dais_cli.delivery import sign_and_send_activity, build_accept_activity, build_reject_activity

console = Console()


@click.group()
def followers():
    """Manage followers."""
    pass


@followers.command()
@click.option('--status', type=click.Choice(['all', 'approved', 'pending', 'rejected']),
              default='all', help='Filter by status')
@click.option('--remote', is_flag=True, help='Query remote database')
def list(status, remote):
    """List followers."""
    console.print(f"[bold blue]Listing {status} followers[/bold blue]\n")

    # Build SQL query
    if status == 'all':
        query = "SELECT follower_actor_id, follower_inbox, status, created_at FROM followers ORDER BY created_at DESC"
    else:
        query = f"SELECT follower_actor_id, follower_inbox, status, created_at FROM followers WHERE status = '{status}' ORDER BY created_at DESC"

    # Find project root (where workers/ directory is)
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    cmd = ["wrangler", "d1", "execute", "dais-social", "--command", query]
    if remote:
        cmd.append("--remote")

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))

        # Parse JSON output
        output = result.stdout
        start = output.find('[')
        end = output.rfind(']') + 1
        data = json.loads(output[start:end])

        results = data[0].get("results", [])

        if not results:
            console.print(f"[dim]No {status} followers found.[/dim]")
            return

        # Display as table
        table = Table(show_header=True, header_style="bold cyan")
        table.add_column("Actor", style="green")
        table.add_column("Status", style="yellow")
        table.add_column("Created", style="dim")

        for row in results:
            actor = row.get("follower_actor_id", "")
            status_val = row.get("status", "")
            created = row.get("created_at", "")

            # Color code status
            if status_val == "approved":
                status_display = f"[green]{status_val}[/green]"
            elif status_val == "pending":
                status_display = f"[yellow]{status_val}[/yellow]"
            else:
                status_display = f"[red]{status_val}[/red]"

            table.add_row(actor, status_display, created)

        console.print(table)
        console.print(f"\n[dim]Total: {len(results)} follower(s)[/dim]")

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Error querying database[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)
    except json.JSONDecodeError as e:
        console.print(f"[red]✗ Error parsing response[/red]")
        sys.exit(1)


@followers.command()
@click.argument('actor')
@click.option('--remote', is_flag=True, help='Update remote database')
def approve(actor, remote):
    """Approve a follow request.

    ACTOR: The ActivityPub actor ID (e.g., @user@instance.social or full URL)
    """
    # Normalize actor format
    if actor.startswith('@'):
        actor = actor[1:]

    # Convert @user@domain to full URL if needed
    # For now, assume it's already a full URL if it starts with https://
    if not actor.startswith('http'):
        console.print(f"[yellow]Note: Actor should be a full URL (e.g., https://mastodon.social/users/alice)[/yellow]")
        console.print(f"[yellow]Using as-is: {actor}[/yellow]")

    console.print(f"[bold blue]Approving follow request from {actor}[/bold blue]\n")

    # Update status in D1
    query = f"UPDATE followers SET status = 'approved' WHERE follower_actor_id = '{actor}'"

    # Find project root
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    cmd = ["wrangler", "d1", "execute", "dais-social", "--command", query]
    if remote:
        cmd.append("--remote")

    try:
        # First, get the follower's inbox URL before updating
        query_follower = f"SELECT follower_inbox, id FROM followers WHERE follower_actor_id = '{actor}'"
        cmd_query = ["wrangler", "d1", "execute", "dais-social", "--command", query_follower]
        if remote:
            cmd_query.append("--remote")

        result_query = subprocess.run(cmd_query, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        output = result_query.stdout
        start = output.find('[')
        end = output.rfind(']') + 1
        data = json.loads(output[start:end])
        follower_data = data[0].get("results", [])[0] if data[0].get("results") else None

        if not follower_data:
            console.print(f"[red]✗ Follower not found[/red]")
            return

        follower_inbox = follower_data['follower_inbox']
        follow_id = follower_data['id']

        # Update status in database
        result = subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        console.print(f"[green]✓[/green] Updated follower status to 'approved'")

        # Send Accept activity
        console.print(f"\n[dim]Sending Accept activity to {follower_inbox}...[/dim]")

        # Build Follow activity object
        our_actor = "https://social.dais.social/users/marc"
        follow_activity = {
            "type": "Follow",
            "id": follow_id,
            "actor": actor,
            "object": our_actor
        }

        # Build Accept activity
        accept_activity = build_accept_activity(our_actor, follow_activity)

        # Send to follower's inbox
        success, status_code = sign_and_send_activity(accept_activity, follower_inbox, our_actor)

        if success:
            console.print(f"[green]✓[/green] Accept activity sent successfully (status: {status_code})")
        else:
            console.print(f"[yellow]⚠[/yellow] Failed to send Accept activity")
            console.print(f"[dim]The follower is approved in the database, but may not see the approval until activity is sent[/dim]")

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Failed to approve follower[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)


@followers.command()
@click.argument('actor')
@click.option('--remote', is_flag=True, help='Update remote database')
def reject(actor, remote):
    """Reject a follow request.

    ACTOR: The ActivityPub actor ID (e.g., @user@instance.social or full URL)
    """
    # Normalize actor format
    if actor.startswith('@'):
        actor = actor[1:]

    if not actor.startswith('http'):
        console.print(f"[yellow]Note: Actor should be a full URL[/yellow]")

    console.print(f"[bold blue]Rejecting follow request from {actor}[/bold blue]\n")

    # Update status in D1
    query = f"UPDATE followers SET status = 'rejected' WHERE follower_actor_id = '{actor}'"

    # Find project root
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    cmd = ["wrangler", "d1", "execute", "dais-social", "--command", query]
    if remote:
        cmd.append("--remote")

    try:
        # First, get the follower's inbox URL before updating
        query_follower = f"SELECT follower_inbox, id FROM followers WHERE follower_actor_id = '{actor}'"
        cmd_query = ["wrangler", "d1", "execute", "dais-social", "--command", query_follower]
        if remote:
            cmd_query.append("--remote")

        result_query = subprocess.run(cmd_query, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        output = result_query.stdout
        start = output.find('[')
        end = output.rfind(']') + 1
        data = json.loads(output[start:end])
        follower_data = data[0].get("results", [])[0] if data[0].get("results") else None

        if not follower_data:
            console.print(f"[red]✗ Follower not found[/red]")
            return

        follower_inbox = follower_data['follower_inbox']
        follow_id = follower_data['id']

        # Update status in database
        result = subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        console.print(f"[green]✓[/green] Updated follower status to 'rejected'")

        # Send Reject activity
        console.print(f"\n[dim]Sending Reject activity to {follower_inbox}...[/dim]")

        # Build Follow activity object
        our_actor = "https://social.dais.social/users/marc"
        follow_activity = {
            "type": "Follow",
            "id": follow_id,
            "actor": actor,
            "object": our_actor
        }

        # Build Reject activity
        reject_activity = build_reject_activity(our_actor, follow_activity)

        # Send to follower's inbox
        success, status_code = sign_and_send_activity(reject_activity, follower_inbox, our_actor)

        if success:
            console.print(f"[green]✓[/green] Reject activity sent successfully (status: {status_code})")
        else:
            console.print(f"[yellow]⚠[/yellow] Failed to send Reject activity")

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Failed to reject follower[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)


@followers.command()
@click.argument('actor')
@click.option('--remote', is_flag=True, help='Update remote database')
def remove(actor, remote):
    """Remove a follower.

    ACTOR: The ActivityPub actor ID (e.g., @user@instance.social or full URL)
    """
    # Normalize actor format
    if actor.startswith('@'):
        actor = actor[1:]

    console.print(f"[bold blue]Removing follower {actor}[/bold blue]\n")

    # Delete from D1
    query = f"DELETE FROM followers WHERE follower_actor_id = '{actor}'"

    # Find project root
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    cmd = ["wrangler", "d1", "execute", "dais-social", "--command", query]
    if remote:
        cmd.append("--remote")

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        console.print(f"[green]✓[/green] Follower removed from database")

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Failed to remove follower[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)
