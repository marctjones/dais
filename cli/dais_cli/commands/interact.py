"""Interaction commands (reply, like, boost)."""

import click
from rich.console import Console
import subprocess
import sys
import json
from pathlib import Path
from datetime import datetime
import uuid
import requests

from dais_cli.config import Config
from dais_cli.delivery import (
    build_create_activity,
    deliver_activity_to_inbox
)

console = Console()


@click.group()
def interact():
    """Interact with other posts (reply, like, boost)."""
    pass


@interact.command()
@click.argument('post_url')
@click.argument('content')
@click.option('--remote', is_flag=True, help='Use remote database and deliver to production')
def reply(post_url, content, remote):
    """Reply to a post.

    POST_URL: The URL of the post to reply to
    CONTENT: Your reply text

    Example:
        dais interact reply https://mastodon.social/@user/123 "Great post!" --remote
    """
    console.print(f"[bold blue]Replying to: {post_url}[/bold blue]\n")
    console.print(f"{content}\n")

    # Load configuration
    config = Config()

    # Determine domain
    if remote:
        activitypub_domain = config.get("server.activitypub_domain", "social.dais.social")
        actor_username = config.get("server.username", "marc")
    else:
        activitypub_domain = "localhost"
        actor_username = config.get("server.username", "marc")

    actor_id = f"https://{activitypub_domain}/users/{actor_username}"

    # Generate reply ID
    reply_uuid = str(uuid.uuid4())[:8]
    timestamp = datetime.utcnow().strftime('%Y%m%d%H%M%S')
    reply_id_path = f"{timestamp}-{reply_uuid}"
    reply_id = f"https://{activitypub_domain}/users/{actor_username}/posts/{reply_id_path}"

    # Fetch the original post to get author's inbox
    console.print("[dim]Fetching original post...[/dim]")
    try:
        response = requests.get(post_url, headers={"Accept": "application/activity+json"}, timeout=10)
        response.raise_for_status()
        original_post = response.json()

        # Get the author's actor URL
        original_author = original_post.get("attributedTo")
        if not original_author:
            console.print("[red]✗ Could not determine post author[/red]")
            sys.exit(1)

        console.print(f"[dim]Post by: {original_author}[/dim]")

        # Fetch author's actor to get inbox
        author_response = requests.get(original_author, headers={"Accept": "application/activity+json"}, timeout=10)
        author_response.raise_for_status()
        author_actor = author_response.json()

        author_inbox = author_actor.get("inbox")
        if not author_inbox:
            console.print("[red]✗ Could not find author's inbox[/red]")
            sys.exit(1)

        console.print(f"[dim]Author inbox: {author_inbox}[/dim]\n")

    except Exception as e:
        console.print(f"[red]✗ Failed to fetch original post: {e}[/red]")
        sys.exit(1)

    # Build reply Note
    published_at = datetime.utcnow().isoformat() + "Z"

    note = {
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Note",
        "id": reply_id,
        "attributedTo": actor_id,
        "content": content,
        "published": published_at,
        "inReplyTo": post_url,  # Mark as reply
        "to": [original_author],
        "cc": ["https://www.w3.org/ns/activitystreams#Public"]
    }

    # Wrap in Create activity
    create_activity = build_create_activity(actor_id, note)

    # Store reply in local database (as a post with in_reply_to)
    console.print("[dim]Saving reply to database...[/dim]")

    content_escaped = content.replace("'", "''")
    to_json = json.dumps([original_author]).replace("'", "''")
    cc_json = json.dumps(["https://www.w3.org/ns/activitystreams#Public"]).replace("'", "''")

    insert_query = f"""
    INSERT INTO posts (id, actor_id, content, visibility, published_at, in_reply_to)
    VALUES ('{reply_id}', '{actor_id}', '{content_escaped}', 'public', '{published_at}', '{post_url}')
    """

    # Get project root for wrangler
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    cmd = ["wrangler", "d1", "execute", "DB", "--command", insert_query]
    if remote:
        cmd.append("--remote")

    try:
        subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        console.print(f"[green]✓[/green] Reply saved to database\n")
    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Failed to save reply[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)

    # Deliver to author's inbox
    console.print(f"[dim]Delivering reply to {original_author}...[/dim]")

    successful = deliver_activity_to_inbox(
        activity=create_activity,
        inbox_url=author_inbox,
        actor_url=actor_id,
    )

    if successful:
        console.print(f"[green]✓[/green] Reply delivered successfully")
    else:
        console.print(f"[yellow]⚠[/yellow] Reply saved but delivery failed")


@interact.command()
@click.argument('post_url')
@click.option('--remote', is_flag=True, help='Use remote database and deliver to production')
def like(post_url, remote):
    """Like a post.

    POST_URL: The URL of the post to like

    Example:
        dais interact like https://mastodon.social/@user/123 --remote
    """
    console.print(f"[bold blue]Liking: {post_url}[/bold blue]\n")

    # Load configuration
    config = Config()

    # Determine domain
    if remote:
        activitypub_domain = config.get("server.activitypub_domain", "social.dais.social")
        actor_username = config.get("server.username", "marc")
    else:
        activitypub_domain = "localhost"
        actor_username = config.get("server.username", "marc")

    actor_id = f"https://{activitypub_domain}/users/{actor_username}"

    # Generate like ID
    like_uuid = str(uuid.uuid4())[:8]
    timestamp = datetime.utcnow().strftime('%Y%m%d%H%M%S')
    like_id = f"https://{activitypub_domain}/users/{actor_username}/likes/{timestamp}-{like_uuid}"

    # Fetch the original post to get author's inbox
    console.print("[dim]Fetching post details...[/dim]")
    try:
        response = requests.get(post_url, headers={"Accept": "application/activity+json"}, timeout=10)
        response.raise_for_status()
        original_post = response.json()

        # Get the author's actor URL
        original_author = original_post.get("attributedTo")
        if not original_author:
            console.print("[red]✗ Could not determine post author[/red]")
            sys.exit(1)

        # Fetch author's actor to get inbox
        author_response = requests.get(original_author, headers={"Accept": "application/activity+json"}, timeout=10)
        author_response.raise_for_status()
        author_actor = author_response.json()

        author_inbox = author_actor.get("inbox")
        if not author_inbox:
            console.print("[red]✗ Could not find author's inbox[/red]")
            sys.exit(1)

    except Exception as e:
        console.print(f"[red]✗ Failed to fetch post: {e}[/red]")
        sys.exit(1)

    # Build Like activity
    like_activity = {
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Like",
        "id": like_id,
        "actor": actor_id,
        "object": post_url
    }

    # Deliver to author's inbox
    console.print(f"[dim]Delivering like to {original_author}...[/dim]")

    successful = deliver_activity_to_inbox(
        activity=like_activity,
        inbox_url=author_inbox,
        actor_url=actor_id,
    )

    if successful:
        console.print(f"[green]✓[/green] Post liked successfully")
    else:
        console.print(f"[red]✗[/red] Failed to deliver like")


@interact.command()
@click.argument('post_url')
@click.option('--remote', is_flag=True, help='Use remote database and deliver to production followers')
def boost(post_url, remote):
    """Boost (reblog/share) a post.

    POST_URL: The URL of the post to boost

    Example:
        dais interact boost https://mastodon.social/@user/123 --remote
    """
    console.print(f"[bold blue]Boosting: {post_url}[/bold blue]\n")

    # Load configuration
    config = Config()

    # Determine domain
    if remote:
        activitypub_domain = config.get("server.activitypub_domain", "social.dais.social")
        actor_username = config.get("server.username", "marc")
    else:
        activitypub_domain = "localhost"
        actor_username = config.get("server.username", "marc")

    actor_id = f"https://{activitypub_domain}/users/{actor_username}"

    # Generate announce ID
    announce_uuid = str(uuid.uuid4())[:8]
    timestamp = datetime.utcnow().strftime('%Y%m%d%H%M%S')
    announce_id = f"https://{activitypub_domain}/users/{actor_username}/announces/{timestamp}-{announce_uuid}"

    # Fetch the original post to get author's inbox
    console.print("[dim]Fetching post details...[/dim]")
    try:
        response = requests.get(post_url, headers={"Accept": "application/activity+json"}, timeout=10)
        response.raise_for_status()
        original_post = response.json()

        # Get the author's actor URL
        original_author = original_post.get("attributedTo")
        if not original_author:
            console.print("[red]✗ Could not determine post author[/red]")
            sys.exit(1)

        # Fetch author's actor to get inbox
        author_response = requests.get(original_author, headers={"Accept": "application/activity+json"}, timeout=10)
        author_response.raise_for_status()
        author_actor = author_response.json()

        author_inbox = author_actor.get("inbox")
        if not author_inbox:
            console.print("[red]✗ Could not find author's inbox[/red]")
            sys.exit(1)

    except Exception as e:
        console.print(f"[red]✗ Failed to fetch post: {e}[/red]")
        sys.exit(1)

    # Build Announce activity
    published_at = datetime.utcnow().isoformat() + "Z"

    announce_activity = {
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Announce",
        "id": announce_id,
        "actor": actor_id,
        "object": post_url,
        "published": published_at,
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "cc": [f"{actor_id}/followers"]
    }

    # Deliver to author's inbox (so they know you boosted)
    console.print(f"[dim]Delivering boost to {original_author}...[/dim]")

    successful_author = deliver_activity_to_inbox(
        activity=announce_activity,
        inbox_url=author_inbox,
        actor_url=actor_id,
    )

    if successful_author:
        console.print(f"[green]✓[/green] Boost delivered to author")
    else:
        console.print(f"[yellow]⚠[/yellow] Failed to deliver to author")

    # Also deliver to your followers
    console.print("[dim]Delivering boost to your followers...[/dim]")

    # Get project root for wrangler
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    followers_query = "SELECT follower_actor_id, follower_inbox FROM followers WHERE status = 'approved'"
    cmd_followers = ["wrangler", "d1", "execute", "DB", "--command", followers_query]
    if remote:
        cmd_followers.append("--remote")

    try:
        result = subprocess.run(cmd_followers, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        output = result.stdout
        start = output.find('[')
        end = output.rfind(']') + 1
        data = json.loads(output[start:end])
        followers = data[0].get("results", [])

        if followers:
            console.print(f"[dim]Delivering to {len(followers)} follower(s)...[/dim]")

            successful_count = 0
            for follower in followers:
                inbox_url = follower.get("follower_inbox")
                if inbox_url and deliver_activity_to_inbox(announce_activity, inbox_url, actor_id):
                    successful_count += 1

            console.print(f"[green]✓[/green] Boost delivered to {successful_count}/{len(followers)} followers")
        else:
            console.print("[dim]No followers to notify[/dim]")

    except subprocess.CalledProcessError as e:
        console.print(f"[yellow]⚠[/yellow] Failed to query followers")


@interact.command()
@click.argument('post_url')
@click.option('--remote', is_flag=True, help='Use remote database and deliver to production')
def unlike(post_url, remote):
    """Unlike a post you previously liked.

    POST_URL: The URL of the post to unlike

    Example:
        dais interact unlike https://mastodon.social/@user/123 --remote
    """
    console.print(f"[bold blue]Unliking: {post_url}[/bold blue]\n")

    # Load configuration
    config = Config()

    # Determine domain
    if remote:
        activitypub_domain = config.get("server.activitypub_domain", "social.dais.social")
        actor_username = config.get("server.username", "marc")
    else:
        activitypub_domain = "localhost"
        actor_username = config.get("server.username", "marc")

    actor_id = f"https://{activitypub_domain}/users/{actor_username}"

    # Generate undo ID
    undo_uuid = str(uuid.uuid4())[:8]
    timestamp = datetime.utcnow().strftime('%Y%m%d%H%M%S')
    undo_id = f"https://{activitypub_domain}/users/{actor_username}/undo/{timestamp}-{undo_uuid}"

    # Fetch the original post to get author's inbox
    console.print("[dim]Fetching post details...[/dim]")
    try:
        response = requests.get(post_url, headers={"Accept": "application/activity+json"}, timeout=10)
        response.raise_for_status()
        original_post = response.json()

        # Get the author's actor URL
        original_author = original_post.get("attributedTo")
        if not original_author:
            console.print("[red]✗ Could not determine post author[/red]")
            sys.exit(1)

        # Fetch author's actor to get inbox
        author_response = requests.get(original_author, headers={"Accept": "application/activity+json"}, timeout=10)
        author_response.raise_for_status()
        author_actor = author_response.json()

        author_inbox = author_actor.get("inbox")
        if not author_inbox:
            console.print("[red]✗ Could not find author's inbox[/red]")
            sys.exit(1)

    except Exception as e:
        console.print(f"[red]✗ Failed to fetch post: {e}[/red]")
        sys.exit(1)

    # Build Undo Like activity
    # Note: We need the original Like activity ID, but for simplicity we'll create a generic one
    like_id = f"https://{activitypub_domain}/users/{actor_username}/likes/{post_url.split('/')[-1]}"

    undo_activity = {
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Undo",
        "id": undo_id,
        "actor": actor_id,
        "object": {
            "type": "Like",
            "id": like_id,
            "actor": actor_id,
            "object": post_url
        }
    }

    # Deliver to author's inbox
    console.print(f"[dim]Delivering unlike to {original_author}...[/dim]")

    successful = deliver_activity_to_inbox(
        activity=undo_activity,
        inbox_url=author_inbox,
        actor_url=actor_id,
    )

    if successful:
        console.print(f"[green]✓[/green] Post unliked successfully")
    else:
        console.print(f"[red]✗[/red] Failed to deliver unlike")
