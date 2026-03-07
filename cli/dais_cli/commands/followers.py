"""Follower management commands."""

import click
from rich.console import Console
from rich.table import Table
import subprocess
import sys
import json
import httpx
from pathlib import Path
from datetime import datetime
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import padding
from cryptography.hazmat.backends import default_backend
import base64

from dais_cli.config import Config

console = Console()


def sign_and_send_activity(activity_type: str, follower_inbox: str, follower_actor: str, follow_id: str):
    """Sign and send an Accept or Reject activity to a follower's inbox."""

    # Build activity
    activity_id = f"https://social.dais.social/activities/{datetime.utcnow().strftime('%Y%m%d%H%M%S')}"
    our_actor = "https://social.dais.social/users/marc"

    activity = {
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": activity_type,
        "id": activity_id,
        "actor": our_actor,
        "object": {
            "type": "Follow",
            "id": follow_id,
            "actor": follower_actor,
            "object": our_actor
        }
    }

    body = json.dumps(activity)

    # Load private key
    private_key_path = Path.home() / ".dais" / "keys" / "private.pem"
    with open(private_key_path, 'rb') as f:
        private_key = serialization.load_pem_private_key(
            f.read(),
            password=None,
            backend=default_backend()
        )

    # Parse URL
    from urllib.parse import urlparse
    parsed = urlparse(follower_inbox)
    host = parsed.netloc
    path = parsed.path

    # Build signature
    date = datetime.utcnow().strftime('%a, %d %b %Y %H:%M:%S GMT')

    import hashlib
    body_hash = hashlib.sha256(body.encode('utf-8')).digest()
    digest = 'SHA-256=' + base64.b64encode(body_hash).decode('utf-8')

    signing_string = f"(request-target): post {path}\nhost: {host}\ndate: {date}\ndigest: {digest}"

    signature_bytes = private_key.sign(
        signing_string.encode('utf-8'),
        padding.PKCS1v15(),
        hashes.SHA256()
    )
    signature_b64 = base64.b64encode(signature_bytes).decode('utf-8')

    key_id = f"{our_actor}#main-key"
    signature_header = (
        f'keyId="{key_id}",'
        f'algorithm="rsa-sha256",'
        f'headers="(request-target) host date digest",'
        f'signature="{signature_b64}"'
    )

    headers = {
        'Date': date,
        'Digest': digest,
        'Signature': signature_header,
        'Content-Type': 'application/activity+json',
        'Accept': 'application/activity+json'
    }

    # Send activity
    try:
        response = httpx.post(follower_inbox, headers=headers, content=body, timeout=30.0)
        return response.status_code in [200, 202], response.status_code
    except Exception as e:
        console.print(f"[red]✗ Error sending activity: {e}[/red]")
        return False, None


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
        success, status_code = sign_and_send_activity("Accept", follower_inbox, actor, follow_id)

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
        success, status_code = sign_and_send_activity("Reject", follower_inbox, actor, follow_id)

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
