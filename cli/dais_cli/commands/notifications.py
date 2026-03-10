"""Notifications management commands."""

import click
from rich.console import Console
from rich.table import Table
import subprocess
import sys
import json
from pathlib import Path

console = Console()


@click.group()
def notifications():
    """Manage notifications."""
    pass


@notifications.command()
@click.option('--limit', type=int, default=20, help='Number of notifications to show')
@click.option('--unread-only', is_flag=True, help='Show only unread notifications')
@click.option('--remote', is_flag=True, help='Query remote database')
def list(limit, unread_only, remote):
    """List your notifications.

    Shows replies, likes, boosts, and mentions from other users.
    """
    filter_text = "unread notifications" if unread_only else "notifications"
    console.print(f"[bold blue]Listing {limit} most recent {filter_text}[/bold blue]\n")

    # Get project root
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    # Build query
    where_clause = "WHERE read = FALSE" if unread_only else ""
    query = f"""
    SELECT id, type, actor_username, actor_display_name, content, created_at, read
    FROM notifications
    {where_clause}
    ORDER BY created_at DESC
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
        notifications_list = data[0].get("results", [])

        if not notifications_list:
            console.print(f"[dim]No {filter_text} found.[/dim]")
            return

        # Display as table
        table = Table(show_header=True, header_style="bold cyan")
        table.add_column("Type", style="yellow")
        table.add_column("From", style="white")
        table.add_column("Content", style="dim")
        table.add_column("When", style="dim")
        table.add_column("Read", style="dim")

        for notif in notifications_list:
            notif_type = notif.get("type", "")
            actor_username = notif.get("actor_username", "")
            actor_display_name = notif.get("actor_display_name", "")
            content = notif.get("content", "")
            created_at = notif.get("created_at", "")
            is_read = notif.get("read", 0)

            # Emoji for type
            type_emoji = {
                "reply": "💬",
                "like": "❤️",
                "boost": "🔁",
                "mention": "@",
                "follow": "👤"
            }.get(notif_type, "📣")

            type_display = f"{type_emoji} {notif_type}"

            # Actor display
            from_display = actor_display_name if actor_display_name else actor_username

            # Content preview
            content_preview = content[:50] + "..." if len(content) > 50 else content

            # Read status
            read_status = "✓" if is_read else "◯"

            table.add_row(
                type_display,
                from_display,
                content_preview,
                created_at,
                read_status
            )

        console.print(table)
        console.print(f"\n[dim]Total: {len(notifications_list)} notification(s)[/dim]")

        # Show unread count if not filtering
        if not unread_only:
            unread_count = sum(1 for n in notifications_list if not n.get("read", 0))
            if unread_count > 0:
                console.print(f"[yellow]Unread: {unread_count}[/yellow]")

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Error querying database[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)
    except json.JSONDecodeError as e:
        console.print(f"[red]✗ Error parsing response[/red]")
        sys.exit(1)


@notifications.command()
@click.option('--remote', is_flag=True, help='Update remote database')
def clear(remote):
    """Mark all notifications as read."""
    console.print("[bold blue]Marking all notifications as read...[/bold blue]\n")

    # Get project root
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    query = "UPDATE notifications SET read = TRUE WHERE read = FALSE"

    cmd = ["wrangler", "d1", "execute", "DB", "--command", query]
    if remote:
        cmd.append("--remote")

    try:
        subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        console.print(f"[green]✓[/green] All notifications marked as read")

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Error updating database[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)


@notifications.command()
@click.option('--remote', is_flag=True, help='Query remote database')
def stats(remote):
    """Show notification statistics."""
    console.print("[bold blue]Notification Statistics[/bold blue]\n")

    # Get project root
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"

    # Query counts by type
    query = """
    SELECT
        type,
        COUNT(*) as count,
        SUM(CASE WHEN read = FALSE THEN 1 ELSE 0 END) as unread_count
    FROM notifications
    GROUP BY type
    ORDER BY count DESC
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
        stats_list = data[0].get("results", [])

        if not stats_list:
            console.print("[dim]No notifications yet.[/dim]")
            return

        # Display as table
        table = Table(show_header=True, header_style="bold cyan")
        table.add_column("Type", style="yellow")
        table.add_column("Total", style="white")
        table.add_column("Unread", style="red")

        total_count = 0
        total_unread = 0

        for stat in stats_list:
            notif_type = stat.get("type", "")
            count = stat.get("count", 0)
            unread_count = stat.get("unread_count", 0)

            total_count += count
            total_unread += unread_count

            # Emoji for type
            type_emoji = {
                "reply": "💬",
                "like": "❤️",
                "boost": "🔁",
                "mention": "@",
                "follow": "👤"
            }.get(notif_type, "📣")

            type_display = f"{type_emoji} {notif_type}"

            table.add_row(
                type_display,
                str(count),
                str(unread_count) if unread_count > 0 else "-"
            )

        console.print(table)
        console.print(f"\n[bold]Total:[/bold] {total_count} notifications")
        if total_unread > 0:
            console.print(f"[yellow]Unread:[/yellow] {total_unread}")

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Error querying database[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)
    except json.JSONDecodeError as e:
        console.print(f"[red]✗ Error parsing response[/red]")
        sys.exit(1)
