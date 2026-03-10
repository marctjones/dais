"""Timeline viewing commands."""
import click
from rich.console import Console
from rich.panel import Panel
from rich.table import Table
from rich.live import Live
from rich.layout import Layout
import requests
from datetime import datetime
from ..delivery import execute_remote_d1

console = Console()


@click.group()
def timeline():
    """View timeline from followed users."""
    pass


@timeline.command()
@click.option('--limit', default=20, help='Number of posts to show')
@click.option('--remote', is_flag=True, help='Use remote database')
def view(limit, remote):
    """View your timeline (posts from followed users)."""
    # Get list of followed users (accepted only)
    query = """
        SELECT target_actor_id, target_inbox
        FROM following
        WHERE status = 'accepted'
        ORDER BY accepted_at DESC
    """

    following = execute_remote_d1(query, remote=remote)

    if not following:
        console.print("[yellow]You are not following anyone yet[/yellow]")
        console.print("Use [cyan]dais follow add @user@instance.social[/cyan] to follow someone")
        return

    console.print(f"Fetching timeline from {len(following)} followed users...")

    # Fetch posts from each followed user
    all_posts = []

    for user in following:
        actor_id = user['target_actor_id']
        # Convert actor ID to outbox URL
        # actor_id format: https://instance.social/users/username
        outbox_url = f"{actor_id}/outbox"

        try:
            response = requests.get(
                outbox_url,
                headers={'Accept': 'application/activity+json'},
                timeout=10
            )

            if response.status_code != 200:
                console.print(f"[dim]Skipping {actor_id} (HTTP {response.status_code})[/dim]")
                continue

            outbox_data = response.json()

            # Get posts from outbox
            items = outbox_data.get('orderedItems', [])
            if isinstance(items, str):
                # If orderedItems is a URL, fetch it
                response = requests.get(items, headers={'Accept': 'application/activity+json'}, timeout=10)
                items = response.json().get('orderedItems', [])

            # Extract Note objects
            for item in items[:limit]:  # Limit per user
                if isinstance(item, dict):
                    item_type = item.get('type')
                    if item_type == 'Note':
                        all_posts.append(item)
                    elif item_type == 'Create':
                        # If it's a Create activity, extract the Note
                        obj = item.get('object', {})
                        if isinstance(obj, dict) and obj.get('type') == 'Note':
                            all_posts.append(obj)

        except requests.RequestException as e:
            console.print(f"[dim]Error fetching from {actor_id}: {e}[/dim]")
            continue

    if not all_posts:
        console.print("[yellow]No posts found in timeline[/yellow]")
        return

    # Sort by published date (newest first)
    all_posts.sort(
        key=lambda p: p.get('published', ''),
        reverse=True
    )

    # Limit to requested number
    all_posts = all_posts[:limit]

    # Display timeline
    console.print(f"\n[bold cyan]Timeline[/bold cyan] ({len(all_posts)} posts)\n")

    for post in all_posts:
        display_post(post)
        console.print()  # Empty line between posts


def display_post(post):
    """Display a single post."""
    # Extract post details
    author_id = post.get('attributedTo', '')
    content = post.get('content', '')
    published = post.get('published', '')
    post_id = post.get('id', '')

    # Parse author handle from actor ID
    parts = author_id.split('/')
    if len(parts) >= 5:
        instance = parts[2]
        username = parts[4]
        author = f"@{username}@{instance}"
    else:
        author = author_id

    # Format timestamp
    try:
        dt = datetime.fromisoformat(published.replace('Z', '+00:00'))
        time_str = dt.strftime('%Y-%m-%d %H:%M')
    except:
        time_str = published[:19] if published else ''

    # Strip HTML tags from content (basic)
    import re
    clean_content = re.sub(r'<[^>]+>', '', content)
    clean_content = re.sub(r'&nbsp;', ' ', clean_content)
    clean_content = re.sub(r'&lt;', '<', clean_content)
    clean_content = re.sub(r'&gt;', '>', clean_content)
    clean_content = re.sub(r'&amp;', '&', clean_content)

    # Truncate if too long
    max_length = 500
    if len(clean_content) > max_length:
        clean_content = clean_content[:max_length] + '...'

    # Check for attachments
    attachments = post.get('attachment', [])
    if attachments and not isinstance(attachments, list):
        attachments = [attachments]

    attachment_str = ""
    if attachments:
        attachment_str = f"\n[dim]📎 {len(attachments)} attachment(s)[/dim]"

    # Display
    console.print(Panel(
        f"[bold]{author}[/bold] · [dim]{time_str}[/dim]\n\n{clean_content}{attachment_str}",
        border_style="blue"
    ))


@timeline.command()
@click.option('--remote', is_flag=True, help='Use remote database')
def refresh(remote):
    """Refresh timeline (same as view for now)."""
    # In the future, this could cache posts locally
    # For now, just call view
    from click import Context
    ctx = Context(view)
    ctx.invoke(view, limit=20, remote=remote)


@timeline.command()
@click.argument('actor_handle')
@click.option('--limit', default=10, help='Number of posts to show')
def user(actor_handle, limit):
    """View posts from a specific user.

    Example: dais timeline user @user@mastodon.social
    """
    # Parse actor handle
    if not actor_handle.startswith('@'):
        console.print("[red]Error: Actor handle must start with @[/red]")
        return

    parts = actor_handle.lstrip('@').split('@')
    if len(parts) != 2:
        console.print("[red]Error: Invalid handle format[/red]")
        return

    username, instance = parts
    actor_id = f"https://{instance}/users/{username}"
    outbox_url = f"{actor_id}/outbox"

    console.print(f"Fetching posts from {actor_handle}...")

    try:
        response = requests.get(
            outbox_url,
            headers={'Accept': 'application/activity+json'},
            timeout=10
        )

        if response.status_code != 200:
            console.print(f"[red]Error: HTTP {response.status_code}[/red]")
            return

        outbox_data = response.json()
        items = outbox_data.get('orderedItems', [])

        if isinstance(items, str):
            # If orderedItems is a URL, fetch it
            response = requests.get(items, headers={'Accept': 'application/activity+json'}, timeout=10)
            items = response.json().get('orderedItems', [])

        # Extract Note objects
        posts = []
        for item in items[:limit]:
            if isinstance(item, dict):
                item_type = item.get('type')
                if item_type == 'Note':
                    posts.append(item)
                elif item_type == 'Create':
                    obj = item.get('object', {})
                    if isinstance(obj, dict) and obj.get('type') == 'Note':
                        posts.append(obj)

        if not posts:
            console.print(f"[yellow]No posts found from {actor_handle}[/yellow]")
            return

        console.print(f"\n[bold cyan]{actor_handle}'s Posts[/bold cyan] ({len(posts)} posts)\n")

        for post in posts:
            display_post(post)
            console.print()

    except requests.RequestException as e:
        console.print(f"[red]Error fetching posts: {e}[/red]")
