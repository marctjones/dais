"""Direct message commands."""
import click
import hashlib
import json
import subprocess
import sys
import uuid
from datetime import datetime
from pathlib import Path
from rich.console import Console
from rich.table import Table
from rich.panel import Panel

from dais_cli.config import Config
from dais_cli.delivery import deliver_activity_to_inbox

console = Console()


def get_conversation_id(participants):
    """Generate deterministic conversation ID from sorted participant list.

    Args:
        participants: List of actor URLs

    Returns:
        Conversation ID (hash of sorted participants)
    """
    sorted_participants = sorted(participants)
    participants_str = "|".join(sorted_participants)
    return hashlib.sha256(participants_str.encode()).hexdigest()[:16]


def resolve_actor(handle, remote=False):
    """Resolve actor handle to actor URL and inbox.

    Args:
        handle: Actor handle like @alice@mastodon.social
        remote: Use remote configuration

    Returns:
        Tuple of (actor_url, inbox_url, display_name)
    """
    import requests

    # Parse handle
    if not handle.startswith('@'):
        console.print(f"[red]Error: Handle must start with @ (e.g., @user@domain)[/red]")
        return None, None, None

    parts = handle.lstrip('@').split('@')
    if len(parts) != 2:
        console.print(f"[red]Error: Invalid handle format. Expected @user@domain[/red]")
        return None, None, None

    username, domain = parts

    try:
        # WebFinger lookup
        webfinger_url = f"https://{domain}/.well-known/webfinger?resource=acct:{username}@{domain}"
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
            console.print(f"[red]Error: Could not find ActivityPub profile for {handle}[/red]")
            return None, None, None

        # Fetch actor profile
        actor_response = requests.get(actor_url, headers={'Accept': 'application/activity+json'}, timeout=10)
        actor_response.raise_for_status()
        actor_data = actor_response.json()

        inbox_url = actor_data.get('inbox')
        display_name = actor_data.get('name') or username

        if not inbox_url:
            console.print(f"[red]Error: Could not find inbox for {handle}[/red]")
            return None, None, None

        return actor_url, inbox_url, display_name

    except Exception as e:
        console.print(f"[red]Error resolving {handle}: {e}[/red]")
        return None, None, None


@click.group()
def dm():
    """Send and manage direct messages."""
    pass


@dm.command()
@click.argument('recipients', nargs=-1, required=True)
@click.argument('message')
@click.option('--remote', is_flag=True, help='Use remote database and deliver to production')
def send(recipients, message, remote):
    """Send a direct message to one or more people.

    RECIPIENTS: One or more handles like @alice@mastodon.social
    MESSAGE: The message text

    Examples:
        dais dm send @alice@mastodon.social "Hey Alice!"
        dais dm send @alice@mastodon.social @bob@example.com "Group message!"
    """
    console.print(f"[bold blue]Sending direct message[/bold blue]\n")

    # Load configuration
    config = Config()

    if remote:
        activitypub_domain = config.get("server.activitypub_domain", "social.dais.social")
        actor_username = config.get("server.username", "marc")
    else:
        activitypub_domain = "localhost"
        actor_username = config.get("server.username", "marc")

    our_actor_id = f"https://{activitypub_domain}/users/{actor_username}"

    # Resolve all recipients
    recipient_actors = []
    recipient_inboxes = {}

    for handle in recipients:
        console.print(f"[dim]Resolving {handle}...[/dim]")
        actor_url, inbox_url, display_name = resolve_actor(handle, remote)

        if not actor_url:
            sys.exit(1)

        recipient_actors.append(actor_url)
        recipient_inboxes[actor_url] = inbox_url
        console.print(f"[green]✓[/green] {display_name} ({actor_url})")

    console.print()

    # Generate message ID
    msg_uuid = str(uuid.uuid4())[:8]
    timestamp = datetime.utcnow().strftime('%Y%m%d%H%M%S')
    msg_id = f"https://{activitypub_domain}/users/{actor_username}/dm/{timestamp}-{msg_uuid}"

    # Determine conversation ID (includes our actor)
    all_participants = sorted([our_actor_id] + recipient_actors)
    conversation_id = get_conversation_id(all_participants)

    # Build ActivityPub DM (Note with specific `to`, no `Public`)
    published_at = datetime.utcnow().isoformat() + "Z"

    dm_note = {
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Note",
        "id": msg_id,
        "attributedTo": our_actor_id,
        "to": recipient_actors,
        "content": message,
        "published": published_at
    }

    dm_activity = {
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Create",
        "id": f"{msg_id}/activity",
        "actor": our_actor_id,
        "to": recipient_actors,
        "object": dm_note
    }

    # Store in database
    console.print("[dim]Saving message to database...[/dim]")

    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    # Escape content for SQL
    content_escaped = message.replace("'", "''")
    participants_json = json.dumps(all_participants).replace("'", "''")

    # Insert or update conversation
    conv_query = f"""
    INSERT INTO conversations (id, participants, last_message_at)
    VALUES ('{conversation_id}', '{participants_json}', '{published_at}')
    ON CONFLICT(id) DO UPDATE SET last_message_at = '{published_at}'
    """

    # Insert message
    msg_query = f"""
    INSERT INTO direct_messages (id, conversation_id, sender_id, content, published_at)
    VALUES ('{msg_id}', '{conversation_id}', '{our_actor_id}', '{content_escaped}', '{published_at}')
    """

    # Insert conversation participants
    participant_queries = []
    for participant in all_participants:
        participant_queries.append(f"""
        INSERT OR IGNORE INTO conversation_participants (conversation_id, actor_id)
        VALUES ('{conversation_id}', '{participant}')
        """)

    all_queries = f"{conv_query}; {msg_query}; " + "; ".join(participant_queries)

    cmd = ["wrangler", "d1", "execute", "DB", "--command", all_queries]
    if remote:
        cmd.append("--remote")

    try:
        subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        console.print(f"[green]✓[/green] Message saved\n")
    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Failed to save message[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)

    # Deliver to all recipients
    console.print(f"[dim]Delivering to {len(recipient_actors)} recipient(s)...[/dim]")

    delivery_count = 0
    for actor_url in recipient_actors:
        inbox_url = recipient_inboxes[actor_url]
        success = deliver_activity_to_inbox(
            activity=dm_activity,
            inbox_url=inbox_url,
            actor_url=our_actor_id
        )

        if success:
            delivery_count += 1
            console.print(f"[green]✓[/green] Delivered to {actor_url}")
        else:
            console.print(f"[yellow]⚠[/yellow] Failed to deliver to {actor_url}")

    console.print()
    if delivery_count == len(recipient_actors):
        console.print(f"[green]✓[/green] Message sent successfully")
    else:
        console.print(f"[yellow]⚠[/yellow] Message sent but some deliveries failed ({delivery_count}/{len(recipient_actors)})")


@dm.command(name='list')
@click.option('--remote', is_flag=True, help='Use remote database')
def list_conversations(remote):
    """List all direct message conversations."""
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    # Query conversations with latest message
    query = """
    SELECT c.id, c.participants, c.last_message_at,
           (SELECT content FROM direct_messages WHERE conversation_id = c.id ORDER BY published_at DESC LIMIT 1) as last_message
    FROM conversations c
    ORDER BY c.last_message_at DESC
    """

    cmd = ["wrangler", "d1", "execute", "DB", "--command", query, "--json"]
    if remote:
        cmd.append("--remote")

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        output = result.stdout

        # Parse JSON output
        data = json.loads(output)
        conversations = data[0].get("results", [])

        if not conversations:
            console.print("[yellow]No conversations yet[/yellow]")
            return

        # Load config to get our actor ID
        config = Config()
        if remote:
            activitypub_domain = config.get("server.activitypub_domain", "social.dais.social")
            actor_username = config.get("server.username", "marc")
        else:
            activitypub_domain = "localhost"
            actor_username = config.get("server.username", "marc")

        our_actor_id = f"https://{activitypub_domain}/users/{actor_username}"

        # Display conversations
        table = Table(title=f"Direct Messages ({len(conversations)})")
        table.add_column("Conversation", style="cyan")
        table.add_column("Last Message", style="white")
        table.add_column("When", style="dim")

        for conv in conversations:
            participants = json.loads(conv['participants'])
            # Remove our actor from display
            other_participants = [p for p in participants if p != our_actor_id]

            # Format participant display
            participant_display = ", ".join([
                p.split('/')[-1] if '/' in p else p
                for p in other_participants
            ])

            last_msg = conv.get('last_message', '')
            last_msg_preview = last_msg[:50] + "..." if len(last_msg) > 50 else last_msg

            last_time = conv.get('last_message_at', '')
            if last_time:
                # Format timestamp
                try:
                    dt = datetime.fromisoformat(last_time.replace('Z', '+00:00'))
                    time_display = dt.strftime("%Y-%m-%d %H:%M")
                except:
                    time_display = last_time[:16]
            else:
                time_display = ""

            table.add_row(participant_display, last_msg_preview, time_display)

        console.print(table)

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Failed to query conversations[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)


@dm.command()
@click.argument('recipient')
@click.option('--remote', is_flag=True, help='Use remote database')
def read(recipient, remote):
    """Read conversation with a specific person.

    RECIPIENT: Handle like @alice@mastodon.social
    """
    # Load configuration
    config = Config()

    if remote:
        activitypub_domain = config.get("server.activitypub_domain", "social.dais.social")
        actor_username = config.get("server.username", "marc")
    else:
        activitypub_domain = "localhost"
        actor_username = config.get("server.username", "marc")

    our_actor_id = f"https://{activitypub_domain}/users/{actor_username}"

    # Resolve recipient
    console.print(f"[dim]Resolving {recipient}...[/dim]")
    actor_url, inbox_url, display_name = resolve_actor(recipient, remote)

    if not actor_url:
        sys.exit(1)

    # Determine conversation ID
    all_participants = sorted([our_actor_id, actor_url])
    conversation_id = get_conversation_id(all_participants)

    # Query messages
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    query = f"""
    SELECT id, sender_id, content, published_at
    FROM direct_messages
    WHERE conversation_id = '{conversation_id}'
    ORDER BY published_at ASC
    """

    cmd = ["wrangler", "d1", "execute", "DB", "--command", query, "--json"]
    if remote:
        cmd.append("--remote")

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        output = result.stdout

        # Parse JSON output
        data = json.loads(output)
        messages = data[0].get("results", [])

        if not messages:
            console.print(f"[yellow]No messages with {recipient}[/yellow]")
            return

        # Display conversation
        console.print(f"\n[bold]Conversation with {display_name}[/bold]\n")

        for msg in messages:
            is_from_us = msg['sender_id'] == our_actor_id
            sender_label = "You" if is_from_us else display_name
            content = msg['content']
            timestamp = msg['published_at'][:19].replace('T', ' ')

            if is_from_us:
                console.print(f"[dim]{timestamp}[/dim] [cyan]{sender_label}:[/cyan] {content}")
            else:
                console.print(f"[dim]{timestamp}[/dim] [green]{sender_label}:[/green] {content}")

        console.print()

        # Update last_read_at
        update_query = f"""
        UPDATE conversation_participants
        SET last_read_at = datetime('now')
        WHERE conversation_id = '{conversation_id}' AND actor_id = '{our_actor_id}'
        """

        cmd_update = ["wrangler", "d1", "execute", "DB", "--command", update_query]
        if remote:
            cmd_update.append("--remote")

        subprocess.run(cmd_update, capture_output=True, text=True, check=True, cwd=str(worker_dir))

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Failed to read conversation[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)
