"""Post management commands."""

import click
from rich.console import Console
from rich.table import Table
import subprocess
import sys
import json
from pathlib import Path
from datetime import datetime
import uuid

from dais_cli.config import Config
from dais_cli.delivery import (
    build_create_activity,
    build_delete_activity
)
from dais_cli.queue_delivery import deliver_dual_protocol_post
from dais_cli.media import (
    upload_to_r2,
    build_attachment_json,
    build_attachment_dict
)

console = Console()


@click.group()
def post():
    """Manage posts."""
    pass


@post.command()
@click.argument('content')
@click.option('--attach', type=click.Path(exists=True), multiple=True,
              help='Attach media file (can be used multiple times)')
@click.option('--alt', type=str, multiple=True,
              help='Alt text for media (provide in same order as --attach)')
@click.option('--visibility', type=click.Choice(['public', 'unlisted', 'followers', 'direct']),
              default='public', help='Post visibility')
@click.option('--protocol', type=click.Choice(['both', 'activitypub', 'atproto']),
              default='both', help='Which protocol(s) to post to')
@click.option('--remote', is_flag=True, help='Use remote database and deliver to production followers')
def create(content, attach, alt, visibility, protocol, remote):
    """Create and publish a post.

    CONTENT: The text content of your post

    Examples:
        dais post create "Hello!" --remote
        dais post create "Check this out!" --attach photo.jpg --alt "Sunset over mountains" --remote
        dais post create "My gallery" --attach img1.jpg --alt "First pic" --attach img2.jpg --alt "Second pic" --remote
    """
    console.print(f"[bold blue]Creating {visibility} post ({protocol})[/bold blue]\n")
    console.print(f"{content}\n")

    # Generate unique post ID
    post_uuid = str(uuid.uuid4())[:8]
    timestamp = datetime.utcnow().strftime('%Y%m%d%H%M%S')
    post_id_path = f"{timestamp}-{post_uuid}"

    # Load configuration
    config = Config()

    # Use localhost for local development, configured domain for remote
    if remote:
        activitypub_domain = config.get("server.activitypub_domain", "social.dais.social")
        actor_username = config.get("server.username", "marc")
    else:
        # Local mode: use hardcoded values matching seed-local-db.sh
        activitypub_domain = "localhost"
        actor_username = "marc"  # Fixed to match local seed data

    actor_id = f"https://{activitypub_domain}/users/{actor_username}"
    post_id = f"https://{activitypub_domain}/users/{actor_username}/posts/{post_id_path}"

    # Get project root
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    # Determine audience
    if visibility == 'public':
        to_audience = ["https://www.w3.org/ns/activitystreams#Public"]
        cc_audience = []
    elif visibility == 'unlisted':
        to_audience = []
        cc_audience = ["https://www.w3.org/ns/activitystreams#Public"]
    elif visibility == 'followers':
        to_audience = [f"{actor_id}/followers"]
        cc_audience = []
    else:  # direct
        to_audience = []
        cc_audience = []

    published_at = datetime.utcnow().isoformat() + "Z"

    # Handle media attachments
    attachment_filenames = []
    media_domain = config.get("cloudflare.r2_public_domain", "social.dais.social")

    if attach:
        console.print(f"[dim]Uploading {len(attach)} media file(s)...[/dim]")
        for file_path in attach:
            filename = upload_to_r2(file_path, remote=remote)
            if filename:
                attachment_filenames.append(filename)
            else:
                console.print(f"[yellow]⚠[/yellow] Skipping {file_path}")

        if not attachment_filenames:
            console.print("[red]✗ All media uploads failed[/red]")
            sys.exit(1)

        console.print(f"[green]✓[/green] Uploaded {len(attachment_filenames)} file(s)\n")

    # Build attachment JSON for database (with alt texts if provided)
    alt_texts = list(alt) if alt else None
    attachments_json = build_attachment_json(attachment_filenames, media_domain, alt_texts) if attachment_filenames else "[]"

    # Insert post to D1 database
    console.print("[dim]Saving post to database...[/dim]")

    # Escape single quotes for SQL
    content_escaped = content.replace("'", "''")
    to_json = json.dumps(to_audience).replace("'", "''")
    cc_json = json.dumps(cc_audience).replace("'", "''") if cc_audience else "[]"
    attachments_escaped = attachments_json.replace("'", "''")

    insert_query = f"""
    INSERT INTO posts (id, actor_id, content, visibility, published_at, media_attachments, protocol)
    VALUES ('{post_id}', '{actor_id}', '{content_escaped}', '{visibility}', '{published_at}', '{attachments_escaped}', '{protocol}')
    """

    cmd = ["wrangler", "d1", "execute", "DB", "--command", insert_query]
    if remote:
        cmd.append("--remote")

    try:
        subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        console.print(f"[green]✓[/green] Post saved to database")
        console.print(f"[dim]Post ID: {post_id}[/dim]\n")
    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Failed to save post[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)

    # Build ActivityPub Note object
    note = {
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Note",
        "id": post_id,
        "attributedTo": actor_id,
        "content": content,
        "published": published_at,
        "to": to_audience,
        "cc": cc_audience
    }

    # Add attachments if present
    if attachment_filenames:
        note["attachment"] = build_attachment_dict(attachment_filenames, media_domain, alt_texts)

    # Wrap in Create activity
    create_activity = build_create_activity(actor_id, note)

    # Queue deliveries for both protocols
    if visibility in ['public', 'unlisted', 'followers']:
        console.print("[dim]Queueing deliveries...[/dim]")

        # Get ActivityPub followers if needed
        followers = []
        if protocol in ['activitypub', 'both']:
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
            except subprocess.CalledProcessError as e:
                console.print(f"[yellow]⚠[/yellow] Failed to query followers")
                console.print(f"[dim]{e.stderr}[/dim]")

        # Deliver to selected protocol(s)
        results = deliver_dual_protocol_post(
            text=content,
            post_id=post_id,
            actor_url=actor_id,
            activity=create_activity,
            followers=followers,
            protocol=protocol,
            remote=remote
        )

        console.print(f"\n[green]✓[/green] Post created and delivered")
        if results['activitypub']['successful'] > 0:
            console.print(f"[dim]ActivityPub: {results['activitypub']['successful']} successful, {results['activitypub']['failed']} failed[/dim]")
        if results['atproto']['success']:
            console.print(f"[dim]AT Protocol: Posted successfully[/dim]")
    else:
        console.print(f"[green]✓[/green] Post created (visibility: {visibility}, no delivery needed)")


@post.command()
@click.option('--limit', type=int, default=20, help='Number of posts to show')
@click.option('--remote', is_flag=True, help='Query remote database')
def list(limit, remote):
    """List your posts."""
    console.print(f"[bold blue]Listing {limit} most recent posts[/bold blue]\n")

    # Get project root
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    # Query posts from database
    query = f"""
    SELECT id, content, visibility, published_at
    FROM posts
    ORDER BY published_at DESC
    LIMIT {limit}
    """

    cmd = ["wrangler", "d1", "execute", "DB", "--command", query]
    if remote:
        cmd.append("--remote")

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        output = result.stdout
        start = output.find('[')
        end = output.rfind(']') + 1
        data = json.loads(output[start:end])
        posts = data[0].get("results", [])

        if not posts:
            console.print("[dim]No posts found.[/dim]")
            return

        # Display as table
        table = Table(show_header=True, header_style="bold cyan")
        table.add_column("ID", style="dim", no_wrap=True)
        table.add_column("Content", style="white")
        table.add_column("Visibility", style="yellow")
        table.add_column("Published", style="dim")

        for post_data in posts:
            post_id = post_data.get("id", "")
            content = post_data.get("content", "")
            visibility = post_data.get("visibility", "")
            published = post_data.get("published_at", "")

            # Extract just the post ID suffix for display
            id_display = post_id.split("/")[-1] if "/" in post_id else post_id

            # Truncate content for display
            content_preview = content[:60] + "..." if len(content) > 60 else content

            # Color code visibility
            if visibility == "public":
                visibility_display = f"[green]{visibility}[/green]"
            elif visibility == "unlisted":
                visibility_display = f"[yellow]{visibility}[/yellow]"
            elif visibility == "followers":
                visibility_display = f"[blue]{visibility}[/blue]"
            else:
                visibility_display = f"[dim]{visibility}[/dim]"

            table.add_row(id_display, content_preview, visibility_display, published)

        console.print(table)
        console.print(f"\n[dim]Total: {len(posts)} post(s)[/dim]")

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Error querying database[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)
    except json.JSONDecodeError as e:
        console.print(f"[red]✗ Error parsing response[/red]")
        sys.exit(1)


@post.command()
@click.argument('post_id')
@click.option('--remote', is_flag=True, help='Delete from remote database and notify production followers')
def delete(post_id, remote):
    """Delete a post.

    POST_ID: The short ID of the post to delete (e.g., "20260107120000-abc123")
    """
    console.print(f"[bold blue]Deleting post {post_id}[/bold blue]\n")

    # Load configuration
    config = Config()

    # Use localhost for local development, configured domain for remote
    if remote:
        activitypub_domain = config.get("server.activitypub_domain", "social.dais.social")
        actor_username = config.get("server.username", "marc")
    else:
        # Local mode: use hardcoded values matching seed-local-db.sh
        activitypub_domain = "localhost"
        actor_username = "marc"  # Fixed to match local seed data

    actor_id = f"https://{activitypub_domain}/users/{actor_username}"

    # Construct full post URL
    full_post_id = f"https://{activitypub_domain}/users/{actor_username}/posts/{post_id}"

    # Get project root
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    # First, verify post exists
    query_post = f"SELECT id FROM posts WHERE id = '{full_post_id}'"
    cmd_query = ["wrangler", "d1", "execute", "DB", "--command", query_post]
    if remote:
        cmd_query.append("--remote")

    try:
        result = subprocess.run(cmd_query, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        output = result.stdout
        start = output.find('[')
        end = output.rfind(']') + 1
        data = json.loads(output[start:end])
        posts = data[0].get("results", [])

        if not posts:
            console.print(f"[red]✗ Post not found: {post_id}[/red]")
            sys.exit(1)

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Error querying post[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)

    # Delete from database
    console.print("[dim]Deleting post from database...[/dim]")

    delete_query = f"DELETE FROM posts WHERE id = '{full_post_id}'"
    cmd = ["wrangler", "d1", "execute", "DB", "--command", delete_query]
    if remote:
        cmd.append("--remote")

    try:
        subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        console.print(f"[green]✓[/green] Post deleted from database\n")
    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Failed to delete post[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)

    # Send Delete activity to followers
    console.print("[dim]Querying approved followers for Delete activity delivery...[/dim]")

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
            console.print(f"[dim]Delivering Delete activity to {len(followers)} approved follower(s)...[/dim]\n")

            # Build Delete activity
            delete_activity = build_delete_activity(actor_id, full_post_id)

            # Deliver to all followers
            successful, failed = deliver_to_followers(
                activity=delete_activity,
                followers=followers,
                actor_url=actor_id,
                verbose=True
            )

            console.print(f"\n[green]✓[/green] Post deleted and Delete activity delivered")
            console.print(f"[dim]Successful deliveries: {successful}/{successful + failed}[/dim]")

            if failed > 0:
                console.print(f"[yellow]⚠[/yellow] Some deliveries failed ({failed})")
        else:
            console.print(f"[dim]No approved followers to notify[/dim]")
            console.print(f"[green]✓[/green] Post deleted successfully")

    except subprocess.CalledProcessError as e:
        console.print(f"[yellow]⚠[/yellow] Failed to query followers for delivery")
        console.print(f"[dim]Post is deleted but Delete activity not sent[/dim]")
