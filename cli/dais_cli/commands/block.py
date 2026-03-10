"""Blocking management commands."""
import click
from rich.console import Console
from rich.table import Table
from rich.panel import Panel
from uuid import uuid4
from datetime import datetime
from ..delivery import execute_remote_d1

console = Console()


@click.group()
def block():
    """Manage blocked users and instances."""
    pass


@block.command()
@click.argument('actor_handle')
@click.option('--reason', help='Reason for blocking')
@click.option('--domain', is_flag=True, help='Block entire domain instead of single user')
@click.option('--remote', is_flag=True, help='Use remote database')
def add(actor_handle, reason, domain, remote):
    """Block a user or domain.

    Examples:
        dais block add @abusive@mastodon.social
        dais block add spam.social --domain --reason "Known spam instance"
    """
    # Parse actor handle or domain
    if domain:
        # Blocking entire domain
        blocked_domain = actor_handle.lstrip('@').split('@')[-1]
        actor_id = f"https://{blocked_domain}/"  # Generic domain ID
        display = blocked_domain
    else:
        # Blocking single user
        if not actor_handle.startswith('@'):
            console.print("[red]Error: Actor handle must start with @ (e.g., @user@instance.social)[/red]")
            return

        # Parse @user@instance.social
        parts = actor_handle.lstrip('@').split('@')
        if len(parts) != 2:
            console.print("[red]Error: Invalid actor handle format. Expected @user@instance.social[/red]")
            return

        username, instance = parts
        actor_id = f"https://{instance}/users/{username}"
        blocked_domain = None
        display = actor_handle

    # Check if already blocked
    check_query = f"""
        SELECT id FROM blocks WHERE actor_id = '{actor_id}'
    """
    existing = execute_remote_d1(check_query, remote=remote)

    if existing:
        console.print(f"[yellow]{display} is already blocked[/yellow]")
        return

    # Insert block
    block_id = str(uuid4())
    created_at = datetime.utcnow().isoformat() + 'Z'

    query = f"""
        INSERT INTO blocks (id, actor_id, blocked_domain, reason, created_at)
        VALUES ('{block_id}', '{actor_id}',
                {f"'{blocked_domain}'" if blocked_domain else 'NULL'},
                {f"'{reason}'" if reason else 'NULL'},
                '{created_at}')
    """

    execute_remote_d1(query, remote=remote)
    console.print(f"[green]✓[/green] Blocked: {display}")

    if reason:
        console.print(f"  Reason: {reason}")


@block.command()
@click.argument('actor_handle')
@click.option('--remote', is_flag=True, help='Use remote database')
def remove(actor_handle, remote):
    """Unblock a user or domain."""
    # Parse actor handle
    if actor_handle.startswith('@'):
        parts = actor_handle.lstrip('@').split('@')
        if len(parts) == 1:
            # Domain only
            blocked_domain = parts[0]
            actor_id = f"https://{blocked_domain}/"
        else:
            username, instance = parts
            actor_id = f"https://{instance}/users/{username}"
    else:
        # Assume domain
        actor_id = f"https://{actor_handle}/"

    query = f"""
        DELETE FROM blocks WHERE actor_id = '{actor_id}'
    """

    execute_remote_d1(query, remote=remote)
    console.print(f"[green]✓[/green] Unblocked: {actor_handle}")


@block.command(name='list')
@click.option('--remote', is_flag=True, help='Use remote database')
def list_blocks(remote):
    """List all blocked users and domains."""
    query = """
        SELECT id, actor_id, blocked_domain, reason, created_at
        FROM blocks
        ORDER BY created_at DESC
    """

    result = execute_remote_d1(query, remote=remote)

    if not result:
        console.print("[yellow]No blocks configured[/yellow]")
        return

    table = Table(title="Blocked Users & Domains")
    table.add_column("Actor/Domain", style="cyan")
    table.add_column("Type", style="blue")
    table.add_column("Reason", style="white")
    table.add_column("Blocked At", style="yellow")

    for row in result:
        if row['blocked_domain']:
            display = row['blocked_domain']
            block_type = "Domain"
        else:
            # Extract handle from actor_id URL
            actor_id = row['actor_id']
            # Parse https://instance.social/users/username
            parts = actor_id.split('/')
            if len(parts) >= 5:
                instance = parts[2]
                username = parts[4]
                display = f"@{username}@{instance}"
            else:
                display = actor_id
            block_type = "User"

        reason = row['reason'] if row['reason'] else '-'
        created_at = row['created_at'][:19]  # Trim milliseconds

        table.add_row(display, block_type, reason, created_at)

    console.print(table)


@block.command()
@click.argument('actor_handle')
@click.option('--remote', is_flag=True, help='Use remote database')
def check(actor_handle, remote):
    """Check if a user or domain is blocked."""
    # Parse actor handle
    if actor_handle.startswith('@'):
        parts = actor_handle.lstrip('@').split('@')
        if len(parts) == 2:
            username, instance = parts
            actor_id = f"https://{instance}/users/{username}"
        else:
            console.print("[red]Error: Invalid actor handle format[/red]")
            return
    else:
        # Assume domain
        instance = actor_handle
        actor_id = f"https://{instance}/"

    # Check both direct block and domain block
    query = f"""
        SELECT id, actor_id, blocked_domain, reason, created_at
        FROM blocks
        WHERE actor_id = '{actor_id}'
           OR blocked_domain = '{instance if '@' in actor_handle else actor_handle}'
    """

    result = execute_remote_d1(query, remote=remote)

    if not result:
        console.print(f"[green]✓[/green] {actor_handle} is not blocked")
        return

    block = result[0]

    if block['blocked_domain']:
        console.print(f"[red]✗[/red] Domain {block['blocked_domain']} is blocked")
    else:
        console.print(f"[red]✗[/red] User {actor_handle} is blocked")

    if block['reason']:
        console.print(f"  Reason: {block['reason']}")
    console.print(f"  Blocked at: {block['created_at']}")
