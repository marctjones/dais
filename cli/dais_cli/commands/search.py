"""Search command for finding posts, users, and content."""

import click
import json
import subprocess
from pathlib import Path
from rich.console import Console
from rich.table import Table
from dais_cli.config import Config

console = Console()


@click.group()
def search():
    """Search posts, users, and content."""
    pass


@search.command()
@click.argument('query')
@click.option('--remote', is_flag=True, help='Search remote database')
@click.option('--limit', default=20, help='Maximum results to show')
def posts(query: str, remote: bool, limit: int):
    """Search for posts by content.

    Searches post content using SQL LIKE queries.

    Example:
        dais search posts "federation" --limit 10
    """
    config = Config()
    config.load()

    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    # SQL query with LIKE search
    query_escaped = query.replace("'", "''")
    sql = f"""
    SELECT id, content, published_at, protocol
    FROM posts
    WHERE content LIKE '%{query_escaped}%'
    ORDER BY published_at DESC
    LIMIT {limit}
    """

    cmd = ["wrangler", "d1", "execute", "DB", "--command", sql]
    if remote:
        cmd.append("--remote")
    else:
        cmd.append("--local")

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            check=True,
            cwd=str(worker_dir)
        )

        # Parse JSON from wrangler output
        start = result.stdout.find('[')
        end = result.stdout.rfind(']') + 1
        if start >= 0 and end > 0:
            data = json.loads(result.stdout[start:end])
            if data and len(data) > 0 and "results" in data[0]:
                results = data[0]["results"]

                if not results:
                    console.print(f"[yellow]No posts found matching '{query}'[/yellow]")
                    return

                # Display results in table
                table = Table(title=f"Posts matching '{query}' ({len(results)} found)")
                table.add_column("Date", style="cyan")
                table.add_column("Protocol", style="magenta")
                table.add_column("Content", style="white", no_wrap=False)

                for post in results:
                    date = post['published_at'][:10]  # Just date
                    protocol = post['protocol']
                    content = post['content'][:100]  # First 100 chars

                    # Highlight search term
                    if query.lower() in content.lower():
                        # Simple highlight (case-insensitive)
                        import re
                        content = re.sub(
                            f'({re.escape(query)})',
                            r'[bold yellow]\1[/bold yellow]',
                            content,
                            flags=re.IGNORECASE
                        )

                    table.add_row(date, protocol, content)

                console.print(table)
                console.print(f"\n[dim]Showing {len(results)} of {len(results)} results[/dim]")

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Search failed: {e}[/red]")
        if e.stderr:
            console.print(f"[dim]{e.stderr}[/dim]")


@search.command()
@click.argument('username')
@click.option('--remote', is_flag=True, help='Search remote database')
def users(username: str, remote: bool):
    """Search for users by username.

    Searches followers and following by username.

    Example:
        dais search users "alice"
    """
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    username_escaped = username.replace("'", "''")

    # Search followers
    sql_followers = f"""
    SELECT actor_username, status, created_at
    FROM followers
    WHERE actor_username LIKE '%{username_escaped}%'
    ORDER BY created_at DESC
    LIMIT 20
    """

    cmd = ["wrangler", "d1", "execute", "DB", "--command", sql_followers]
    if remote:
        cmd.append("--remote")
    else:
        cmd.append("--local")

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            check=True,
            cwd=str(worker_dir)
        )

        start = result.stdout.find('[')
        end = result.stdout.rfind(']') + 1
        if start >= 0 and end > 0:
            data = json.loads(result.stdout[start:end])
            if data and len(data) > 0 and "results" in data[0]:
                results = data[0]["results"]

                if not results:
                    console.print(f"[yellow]No users found matching '{username}'[/yellow]")
                    return

                table = Table(title=f"Users matching '{username}'")
                table.add_column("Username", style="cyan")
                table.add_column("Status", style="magenta")
                table.add_column("Since", style="dim")

                for user in results:
                    status_color = {
                        'accepted': '[green]accepted[/green]',
                        'pending': '[yellow]pending[/yellow]',
                        'rejected': '[red]rejected[/red]'
                    }.get(user['status'], user['status'])

                    table.add_row(
                        user['actor_username'],
                        status_color,
                        user['created_at'][:10]
                    )

                console.print(table)

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Search failed: {e}[/red]")


@search.command()
@click.argument('query')
@click.option('--remote', is_flag=True, help='Search remote database')
@click.option('--limit', default=20, help='Maximum results to show')
def replies(query: str, remote: bool, limit: int):
    """Search for replies by content.

    Example:
        dais search replies "great post"
    """
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    query_escaped = query.replace("'", "''")
    sql = f"""
    SELECT r.actor_username, r.content, r.published_at, r.moderation_status,
           p.content as parent_content
    FROM replies r
    JOIN posts p ON r.post_id = p.id
    WHERE r.content LIKE '%{query_escaped}%'
    ORDER BY r.published_at DESC
    LIMIT {limit}
    """

    cmd = ["wrangler", "d1", "execute", "DB", "--command", sql]
    if remote:
        cmd.append("--remote")
    else:
        cmd.append("--local")

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            check=True,
            cwd=str(worker_dir)
        )

        start = result.stdout.find('[')
        end = result.stdout.rfind(']') + 1
        if start >= 0 and end > 0:
            data = json.loads(result.stdout[start:end])
            if data and len(data) > 0 and "results" in data[0]:
                results = data[0]["results"]

                if not results:
                    console.print(f"[yellow]No replies found matching '{query}'[/yellow]")
                    return

                table = Table(title=f"Replies matching '{query}'")
                table.add_column("From", style="cyan")
                table.add_column("Reply", style="white", no_wrap=False)
                table.add_column("Status", style="magenta")

                for reply in results:
                    status_icon = {
                        'approved': '✓',
                        'pending': '⏳',
                        'rejected': '✗'
                    }.get(reply['moderation_status'], '?')

                    table.add_row(
                        reply['actor_username'],
                        reply['content'][:80],
                        status_icon
                    )

                console.print(table)

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Search failed: {e}[/red]")


@search.command()
@click.option('--remote', is_flag=True, help='Search remote database')
def stats(remote: bool):
    """Show search-related statistics.

    Shows total counts of searchable content.
    """
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    sql = """
    SELECT
        (SELECT COUNT(*) FROM posts) as total_posts,
        (SELECT COUNT(*) FROM replies) as total_replies,
        (SELECT COUNT(*) FROM followers WHERE status='accepted') as total_followers,
        (SELECT COUNT(*) FROM notifications) as total_notifications
    """

    cmd = ["wrangler", "d1", "execute", "DB", "--command", sql]
    if remote:
        cmd.append("--remote")
    else:
        cmd.append("--local")

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            check=True,
            cwd=str(worker_dir)
        )

        start = result.stdout.find('[')
        end = result.stdout.rfind(']') + 1
        if start >= 0 and end > 0:
            data = json.loads(result.stdout[start:end])
            if data and len(data) > 0 and "results" in data[0]:
                stats = data[0]["results"][0]

                console.print("\n[bold]Searchable Content Statistics[/bold]\n")
                console.print(f"  Posts: [cyan]{stats['total_posts']}[/cyan]")
                console.print(f"  Replies: [cyan]{stats['total_replies']}[/cyan]")
                console.print(f"  Followers: [cyan]{stats['total_followers']}[/cyan]")
                console.print(f"  Notifications: [cyan]{stats['total_notifications']}[/cyan]")
                console.print(f"\n  Total: [bold cyan]{sum(stats.values())}[/bold cyan] items\n")

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Failed to get stats: {e}[/red]")
