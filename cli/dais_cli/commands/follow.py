"""Following management commands."""
import click
from rich.console import Console
from rich.table import Table
from uuid import uuid4
from datetime import datetime
import json
import requests
from ..delivery import execute_remote_d1, get_actor_info, send_activity

console = Console()


@click.group()
def follow():
    """Manage users you follow."""
    pass


@follow.command()
@click.argument('actor_handle')
@click.option('--remote', is_flag=True, help='Use remote database')
def add(actor_handle, remote):
    """Follow a user.

    Example: dais follow add @user@mastodon.social
    """
    # Parse actor handle
    if not actor_handle.startswith('@'):
        console.print("[red]Error: Actor handle must start with @ (e.g., @user@instance.social)[/red]")
        return

    parts = actor_handle.lstrip('@').split('@')
    if len(parts) != 2:
        console.print("[red]Error: Invalid actor handle format. Expected @user@instance.social[/red]")
        return

    username, instance = parts

    # Fetch actor info via WebFinger
    console.print(f"Looking up {actor_handle}...")

    try:
        # WebFinger lookup
        webfinger_url = f"https://{instance}/.well-known/webfinger?resource=acct:{username}@{instance}"
        response = requests.get(webfinger_url, headers={'Accept': 'application/jrd+json'}, timeout=10)
        response.raise_for_status()
        webfinger_data = response.json()

        # Find ActivityPub profile link
        actor_url = None
        for link in webfinger_data.get('links', []):
            if link.get('rel') == 'self' and link.get('type') == 'application/activity+json':
                actor_url = link.get('href')
                break

        if not actor_url:
            console.print("[red]Error: Could not find ActivityPub profile[/red]")
            return

        # Fetch actor profile
        response = requests.get(actor_url, headers={'Accept': 'application/activity+json'}, timeout=10)
        response.raise_for_status()
        actor_data = response.json()

        target_inbox = actor_data.get('inbox')
        if not target_inbox:
            console.print("[red]Error: Actor profile missing inbox URL[/red]")
            return

    except requests.RequestException as e:
        console.print(f"[red]Error: Failed to lookup actor: {e}[/red]")
        return

    # Get our actor info
    our_actor = get_actor_info(remote=remote)
    our_actor_id = f"https://social.dais.social/users/{our_actor['username']}"

    # Check if already following
    check_query = f"""
        SELECT id, status FROM following
        WHERE actor_id = '{our_actor_id}' AND target_actor_id = '{actor_url}'
    """
    existing = execute_remote_d1(check_query, remote=remote)

    if existing:
        status = existing[0]['status']
        if status == 'accepted':
            console.print(f"[yellow]You are already following {actor_handle}[/yellow]")
        elif status == 'pending':
            console.print(f"[yellow]Follow request already sent to {actor_handle} (pending)[/yellow]")
        elif status == 'rejected':
            console.print(f"[yellow]{actor_handle} previously rejected your follow request[/yellow]")
        return

    # Create Follow activity
    follow_id = str(uuid4())
    activity_id = f"{our_actor_id}/activities/{follow_id}"
    created_at = datetime.utcnow().isoformat() + 'Z'

    follow_activity = {
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": activity_id,
        "type": "Follow",
        "actor": our_actor_id,
        "object": actor_url
    }

    # Store in database
    insert_query = f"""
        INSERT INTO following (id, actor_id, target_actor_id, target_inbox, status, created_at)
        VALUES ('{activity_id}', '{our_actor_id}', '{actor_url}', '{target_inbox}', 'pending', '{created_at}')
    """
    execute_remote_d1(insert_query, remote=remote)

    # Send Follow activity
    console.print(f"Sending follow request to {actor_handle}...")

    try:
        send_activity(target_inbox, follow_activity, remote=remote)
        console.print(f"[green]✓[/green] Follow request sent to {actor_handle}")
        console.print("  Status: Pending (waiting for acceptance)")
    except Exception as e:
        console.print(f"[red]Error sending follow request: {e}[/red]")
        # Clean up database entry
        execute_remote_d1(f"DELETE FROM following WHERE id = '{activity_id}'", remote=remote)


@follow.command()
@click.argument('actor_handle')
@click.option('--remote', is_flag=True, help='Use remote database')
def remove(actor_handle, remote):
    """Unfollow a user."""
    # Parse actor handle
    if not actor_handle.startswith('@'):
        console.print("[red]Error: Actor handle must start with @[/red]")
        return

    parts = actor_handle.lstrip('@').split('@')
    if len(parts) != 2:
        console.print("[red]Error: Invalid actor handle format[/red]")
        return

    username, instance = parts
    actor_url = f"https://{instance}/users/{username}"

    # Get our actor info
    our_actor = get_actor_info(remote=remote)
    our_actor_id = f"https://social.dais.social/users/{our_actor['username']}"

    # Check if we're following them
    check_query = f"""
        SELECT id, target_inbox, status FROM following
        WHERE actor_id = '{our_actor_id}' AND target_actor_id = '{actor_url}'
    """
    result = execute_remote_d1(check_query, remote=remote)

    if not result:
        console.print(f"[yellow]You are not following {actor_handle}[/yellow]")
        return

    follow_record = result[0]
    follow_id = follow_record['id']
    target_inbox = follow_record['target_inbox']

    # Create Undo activity
    undo_id = str(uuid4())
    undo_activity_id = f"{our_actor_id}/activities/{undo_id}"

    undo_activity = {
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": undo_activity_id,
        "type": "Undo",
        "actor": our_actor_id,
        "object": {
            "id": follow_id,
            "type": "Follow",
            "actor": our_actor_id,
            "object": actor_url
        }
    }

    # Send Undo activity
    try:
        send_activity(target_inbox, undo_activity, remote=remote)
        console.print(f"[green]✓[/green] Unfollowed {actor_handle}")
    except Exception as e:
        console.print(f"[red]Error sending unfollow: {e}[/red]")

    # Remove from database
    delete_query = f"""
        DELETE FROM following
        WHERE actor_id = '{our_actor_id}' AND target_actor_id = '{actor_url}'
    """
    execute_remote_d1(delete_query, remote=remote)


@follow.command(name='list')
@click.option('--status', type=click.Choice(['pending', 'accepted', 'rejected']),
              help='Filter by status')
@click.option('--remote', is_flag=True, help='Use remote database')
def list_following(status, remote):
    """List users you follow."""
    # Build query
    where_clause = f"WHERE status = '{status}'" if status else ""

    query = f"""
        SELECT id, target_actor_id, target_inbox, status, created_at, accepted_at
        FROM following
        {where_clause}
        ORDER BY created_at DESC
    """

    result = execute_remote_d1(query, remote=remote)

    if not result:
        if status:
            console.print(f"[yellow]No {status} follows[/yellow]")
        else:
            console.print("[yellow]You are not following anyone[/yellow]")
        return

    table = Table(title=f"Following ({len(result)})")
    table.add_column("User", style="cyan")
    table.add_column("Instance", style="blue")
    table.add_column("Status", style="yellow")
    table.add_column("Since", style="white")

    for row in result:
        # Parse actor URL
        target_actor_id = row['target_actor_id']
        parts = target_actor_id.split('/')
        if len(parts) >= 5:
            instance = parts[2]
            username = parts[4]
            display = f"@{username}@{instance}"
        else:
            display = target_actor_id
            instance = ""

        status_display = row['status']
        if status_display == 'accepted':
            status_display = "[green]✓ Accepted[/green]"
        elif status_display == 'pending':
            status_display = "[yellow]⏳ Pending[/yellow]"
        elif status_display == 'rejected':
            status_display = "[red]✗ Rejected[/red]"

        since = row['accepted_at'] if row['accepted_at'] else row['created_at']
        since = since[:19] if since else ''

        table.add_row(display, instance, status_display, since)

    console.print(table)


@follow.command()
@click.option('--remote', is_flag=True, help='Use remote database')
def stats(remote):
    """Show following statistics."""
    query = """
        SELECT
            COUNT(*) as total,
            SUM(CASE WHEN status = 'accepted' THEN 1 ELSE 0 END) as accepted,
            SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) as pending,
            SUM(CASE WHEN status = 'rejected' THEN 1 ELSE 0 END) as rejected
        FROM following
    """

    result = execute_remote_d1(query, remote=remote)

    if not result or result[0]['total'] == 0:
        console.print("[yellow]No following data[/yellow]")
        return

    stats = result[0]

    console.print(f"""
[bold cyan]Following Statistics:[/bold cyan]

Total Follow Requests: {stats['total']}
  • Accepted: {stats['accepted']} ({stats['accepted']/stats['total']*100:.1f}%)
  • Pending: {stats['pending']} ({stats['pending']/stats['total']*100:.1f}%)
  • Rejected: {stats['rejected']} ({stats['rejected']/stats['total']*100:.1f}%)
    """.strip())
