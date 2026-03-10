"""Bluesky (AT Protocol) integration commands."""
import click
from rich.console import Console
from rich.table import Table
from rich.panel import Panel
from rich.prompt import Prompt, Confirm
from pathlib import Path
import json
from datetime import datetime
from atproto import Client, models
from atproto.exceptions import AtProtocolError
from dais_cli.config import get_dais_dir

console = Console()

# Configuration file for Bluesky credentials
BLUESKY_CONFIG = get_dais_dir() / "bluesky.json"


def get_bluesky_client():
    """Get authenticated Bluesky client."""
    if not BLUESKY_CONFIG.exists():
        console.print("[red]Not logged in to Bluesky. Run: dais bluesky login[/red]")
        raise click.Abort()

    with open(BLUESKY_CONFIG) as f:
        config = json.load(f)

    client = Client()
    try:
        client.login(config['handle'], config['password'])
        return client
    except AtProtocolError as e:
        console.print(f"[red]Login failed: {e}[/red]")
        console.print("[yellow]Try: dais bluesky login[/yellow]")
        raise click.Abort()


def save_credentials(handle, password):
    """Save Bluesky credentials."""
    BLUESKY_CONFIG.parent.mkdir(parents=True, exist_ok=True)
    with open(BLUESKY_CONFIG, 'w') as f:
        json.dump({'handle': handle, 'password': password}, f)
    BLUESKY_CONFIG.chmod(0o600)  # Secure permissions


@click.group()
def bluesky():
    """Bluesky (AT Protocol) integration."""
    pass


@bluesky.command()
def login():
    """Login to Bluesky."""
    console.print("[cyan]Bluesky Login[/cyan]\n")

    handle = Prompt.ask("Handle (e.g., user.bsky.social)")
    password = Prompt.ask("Password (or App Password)", password=True)

    console.print("\nAuthenticating...")

    client = Client()
    try:
        client.login(handle, password)
        console.print(f"[green]✓[/green] Logged in as @{handle}")

        # Save credentials
        save_credentials(handle, password)
        console.print(f"[dim]Credentials saved to {BLUESKY_CONFIG}[/dim]")

        # Show profile info
        profile = client.get_profile(handle)
        console.print(f"\n[bold]{profile.display_name or handle}[/bold]")
        if profile.description:
            console.print(f"{profile.description}")
        console.print(f"Followers: {profile.followers_count} | Following: {profile.follows_count}")

    except AtProtocolError as e:
        console.print(f"[red]Login failed: {e}[/red]")
        raise click.Abort()


@bluesky.command()
def logout():
    """Logout from Bluesky."""
    if not BLUESKY_CONFIG.exists():
        console.print("[yellow]Not logged in[/yellow]")
        return

    if Confirm.ask("Logout from Bluesky?"):
        BLUESKY_CONFIG.unlink()
        console.print("[green]✓[/green] Logged out")


@bluesky.command()
def whoami():
    """Show current Bluesky user."""
    if not BLUESKY_CONFIG.exists():
        console.print("[red]Not logged in. Run: dais bluesky login[/red]")
        return

    client = get_bluesky_client()
    profile = client.get_profile(client.me.handle)

    console.print(Panel(f"""
[bold]Handle:[/bold] @{profile.handle}
[bold]Display Name:[/bold] {profile.display_name or 'Not set'}
[bold]DID:[/bold] {profile.did}

[bold]Bio:[/bold]
{profile.description or 'No bio'}

[bold]Stats:[/bold]
• Posts: {profile.posts_count}
• Followers: {profile.followers_count}
• Following: {profile.follows_count}
    """.strip(), title="Bluesky Profile"))


@bluesky.group()
def post():
    """Manage Bluesky posts."""
    pass


@post.command()
@click.argument('text')
@click.option('--reply-to', help='URI of post to reply to')
def create(text, reply_to):
    """Create a Bluesky post.

    Example: dais bluesky post create "Hello Bluesky!"
    """
    client = get_bluesky_client()

    try:
        if reply_to:
            # Parse reply URI to get repo and rkey
            # URI format: at://did:plc:xxx/app.bsky.feed.post/rkey
            parts = reply_to.replace('at://', '').split('/')
            if len(parts) >= 3:
                parent_ref = models.create_strong_ref(
                    models.ComAtprotoRepoStrongRef.Main(
                        uri=reply_to,
                        cid='bafyreib...'  # Placeholder - need to fetch actual CID
                    )
                )
                root_ref = parent_ref  # For simple replies, root = parent

                response = client.send_post(
                    text=text,
                    reply_to=models.AppBskyFeedPost.ReplyRef(parent=parent_ref, root=root_ref)
                )
            else:
                console.print("[red]Invalid reply-to URI format[/red]")
                return
        else:
            response = client.send_post(text=text)

        console.print(f"[green]✓[/green] Posted to Bluesky")
        console.print(f"URI: {response.uri}")

    except AtProtocolError as e:
        console.print(f"[red]Error: {e}[/red]")


@post.command(name='list')
@click.option('--handle', help='Handle to fetch posts from (default: your own)')
@click.option('--limit', default=20, help='Number of posts to show')
def list_posts(handle, limit):
    """List Bluesky posts."""
    client = get_bluesky_client()

    target_handle = handle or client.me.handle

    try:
        feed = client.get_author_feed(target_handle, limit=limit)

        if not feed.feed:
            console.print(f"[yellow]No posts found from @{target_handle}[/yellow]")
            return

        console.print(f"\n[bold cyan]@{target_handle}'s Posts[/bold cyan] ({len(feed.feed)} posts)\n")

        for item in feed.feed:
            post = item.post
            record = post.record

            # Format timestamp
            created_at = datetime.fromisoformat(record.created_at.replace('Z', '+00:00'))
            time_str = created_at.strftime('%Y-%m-%d %H:%M')

            # Engagement stats
            likes = post.like_count or 0
            reposts = post.repost_count or 0
            replies = post.reply_count or 0

            # Display post
            console.print(Panel(
                f"{record.text}\n\n"
                f"[dim]💬 {replies} · 🔁 {reposts} · ❤️ {likes} · {time_str}[/dim]",
                border_style="blue"
            ))
            console.print()

    except AtProtocolError as e:
        console.print(f"[red]Error: {e}[/red]")


@bluesky.group()
def timeline():
    """View Bluesky timeline."""
    pass


@timeline.command()
@click.option('--limit', default=30, help='Number of posts to show')
@click.option('--algorithm', default='reverse-chronological',
              help='Timeline algorithm (reverse-chronological, etc.)')
def view(limit, algorithm):
    """View your Bluesky timeline."""
    client = get_bluesky_client()

    try:
        feed = client.get_timeline(limit=limit, algorithm=algorithm)

        if not feed.feed:
            console.print("[yellow]No posts in timeline[/yellow]")
            console.print("Follow some users first!")
            return

        console.print(f"\n[bold cyan]Timeline[/bold cyan] ({len(feed.feed)} posts)\n")

        for item in feed.feed:
            post = item.post
            record = post.record
            author = post.author

            # Format timestamp
            created_at = datetime.fromisoformat(record.created_at.replace('Z', '+00:00'))
            time_str = created_at.strftime('%Y-%m-%d %H:%M')

            # Engagement stats
            likes = post.like_count or 0
            reposts = post.repost_count or 0
            replies = post.reply_count or 0

            # Handle reposts
            if item.reason and item.reason.py_type == 'app.bsky.feed.defs#reasonRepost':
                reposted_by = item.reason.by
                header = f"🔁 Reposted by @{reposted_by.handle}"
            else:
                header = ""

            # Display post
            display_name = author.display_name or author.handle

            console.print(Panel(
                f"[bold]@{author.handle}[/bold] ({display_name}) · [dim]{time_str}[/dim]\n\n"
                f"{record.text}\n\n"
                f"[dim]💬 {replies} · 🔁 {reposts} · ❤️ {likes}[/dim]",
                title=header if header else None,
                border_style="blue"
            ))
            console.print()

    except AtProtocolError as e:
        console.print(f"[red]Error: {e}[/red]")


@bluesky.group()
def follow():
    """Manage follows on Bluesky."""
    pass


@follow.command()
@click.argument('handle')
def add(handle):
    """Follow a Bluesky user.

    Example: dais bluesky follow add alice.bsky.social
    """
    client = get_bluesky_client()

    # Remove @ if present
    handle = handle.lstrip('@')

    try:
        # Get the user's DID first
        profile = client.get_profile(handle)

        # Follow them
        client.follow(profile.did)

        console.print(f"[green]✓[/green] Now following @{handle}")

    except AtProtocolError as e:
        console.print(f"[red]Error: {e}[/red]")


@follow.command()
@click.argument('handle')
def remove(handle):
    """Unfollow a Bluesky user."""
    client = get_bluesky_client()

    handle = handle.lstrip('@')

    try:
        profile = client.get_profile(handle)

        # Get follow record to delete
        follows = client.get_follows(client.me.did, limit=100)
        follow_record = None

        for follow in follows.follows:
            if follow.did == profile.did:
                follow_record = follow
                break

        if not follow_record:
            console.print(f"[yellow]Not following @{handle}[/yellow]")
            return

        # Unfollow (delete the follow record)
        client.unfollow(profile.did)

        console.print(f"[green]✓[/green] Unfollowed @{handle}")

    except AtProtocolError as e:
        console.print(f"[red]Error: {e}[/red]")


@follow.command(name='list')
@click.option('--followers', is_flag=True, help='Show followers instead of following')
@click.option('--limit', default=50, help='Number to show')
def list_follows(followers, limit):
    """List who you follow or your followers."""
    client = get_bluesky_client()

    try:
        if followers:
            result = client.get_followers(client.me.did, limit=limit)
            users = result.followers
            title = f"Followers ({len(users)})"
        else:
            result = client.get_follows(client.me.did, limit=limit)
            users = result.follows
            title = f"Following ({len(users)})"

        if not users:
            console.print(f"[yellow]No {title.lower()}[/yellow]")
            return

        table = Table(title=title)
        table.add_column("Handle", style="cyan")
        table.add_column("Display Name", style="white")
        table.add_column("Followers", style="blue")

        for user in users:
            display_name = user.display_name or ''
            followers = str(user.followers_count) if hasattr(user, 'followers_count') else '-'

            table.add_row(
                f"@{user.handle}",
                display_name,
                followers
            )

        console.print(table)

    except AtProtocolError as e:
        console.print(f"[red]Error: {e}[/red]")


@bluesky.command()
@click.argument('post_uri')
def like(post_uri):
    """Like a Bluesky post.

    Example: dais bluesky like at://did:plc:xxx/app.bsky.feed.post/xxx
    """
    client = get_bluesky_client()

    try:
        # Parse URI
        parts = post_uri.replace('at://', '').split('/')
        if len(parts) < 3:
            console.print("[red]Invalid post URI[/red]")
            return

        # Like the post
        client.like(post_uri, parts[-2])  # cid placeholder

        console.print("[green]✓[/green] Liked post")

    except AtProtocolError as e:
        console.print(f"[red]Error: {e}[/red]")


@bluesky.command()
@click.argument('post_uri')
def repost(post_uri):
    """Repost (quote/share) a Bluesky post.

    Example: dais bluesky repost at://did:plc:xxx/app.bsky.feed.post/xxx
    """
    client = get_bluesky_client()

    try:
        # Repost
        client.repost(post_uri, 'cid-placeholder')

        console.print("[green]✓[/green] Reposted")

    except AtProtocolError as e:
        console.print(f"[red]Error: {e}[/red]")


@bluesky.command()
@click.argument('handle')
def profile(handle):
    """View a Bluesky user's profile.

    Example: dais bluesky profile alice.bsky.social
    """
    client = get_bluesky_client()

    handle = handle.lstrip('@')

    try:
        profile = client.get_profile(handle)

        console.print(Panel(f"""
[bold]@{profile.handle}[/bold]
{profile.display_name or 'No display name'}

[bold]DID:[/bold] {profile.did}

[bold]Bio:[/bold]
{profile.description or 'No bio'}

[bold]Stats:[/bold]
• Posts: {profile.posts_count}
• Followers: {profile.followers_count}
• Following: {profile.follows_count}

[bold]Joined:[/bold] {profile.created_at if hasattr(profile, 'created_at') else 'Unknown'}
        """.strip(), title="Bluesky Profile"))

    except AtProtocolError as e:
        console.print(f"[red]Error: {e}[/red]")


@bluesky.command()
@click.option('--cross-post', is_flag=True, help='Also post to ActivityPub/Mastodon')
@click.argument('text')
def quick_post(text, cross_post):
    """Quick post to Bluesky (and optionally ActivityPub).

    Example: dais bluesky quick-post "Hello world!"
    Example: dais bluesky quick-post "Hello both!" --cross-post
    """
    # Post to Bluesky
    client = get_bluesky_client()

    try:
        response = client.send_post(text=text)
        console.print(f"[green]✓[/green] Posted to Bluesky")
        console.print(f"URI: {response.uri}")

        # Cross-post to ActivityPub if requested
        if cross_post:
            from ..delivery import execute_remote_d1, get_actor_info
            from uuid import uuid4
            from datetime import datetime

            console.print("\nCross-posting to ActivityPub...")

            actor = get_actor_info(remote=True)
            post_id = f"{datetime.utcnow().strftime('%Y%m%d%H%M%S')}-{uuid4().hex[:8]}"
            published = datetime.utcnow().isoformat() + 'Z'
            actor_id = f"https://social.dais.social/users/{actor['username']}"

            query = f"""
                INSERT INTO posts (id, actor_id, content, visibility, published_at)
                VALUES ('{post_id}', '{actor_id}', '{text.replace("'", "''")}', 'public', '{published}')
            """

            execute_remote_d1(query, remote=True)
            console.print(f"[green]✓[/green] Posted to ActivityPub")
            console.print(f"URL: https://social.dais.social/users/{actor['username']}/posts/{post_id}")

    except AtProtocolError as e:
        console.print(f"[red]Bluesky error: {e}[/red]")
    except Exception as e:
        console.print(f"[red]ActivityPub error: {e}[/red]")
