"""Moderation management commands."""
import click
from rich.console import Console
from rich.table import Table
from rich.panel import Panel
import json
from datetime import datetime
from ..delivery import execute_remote_d1

console = Console()


@click.group()
def moderation():
    """Manage content moderation."""
    pass


@moderation.command()
@click.option('--status', type=click.Choice(['pending', 'hidden', 'rejected', 'approved']),
              help='Filter by moderation status')
@click.option('--limit', default=20, help='Number of items to show')
@click.option('--remote', is_flag=True, help='Use remote database')
def review(status, limit, remote):
    """Review moderated content."""
    # Build query based on filters
    if status:
        query = f"""
            SELECT r.id, r.post_id, r.actor_username, r.content,
                   r.moderation_status, r.moderation_score, r.moderation_flags,
                   r.hidden, r.published_at
            FROM replies r
            WHERE r.moderation_status = '{status}'
            ORDER BY r.published_at DESC
            LIMIT {limit}
        """
    else:
        # Show only items that need review (not approved)
        query = f"""
            SELECT r.id, r.post_id, r.actor_username, r.content,
                   r.moderation_status, r.moderation_score, r.moderation_flags,
                   r.hidden, r.published_at
            FROM replies r
            WHERE r.moderation_status != 'approved'
            ORDER BY r.published_at DESC
            LIMIT {limit}
        """

    result = execute_remote_d1(query, remote=remote)

    if not result:
        console.print("[yellow]No moderated content to review[/yellow]")
        return

    table = Table(title="Moderated Content Review")
    table.add_column("ID", style="cyan", no_wrap=True)
    table.add_column("Author", style="blue")
    table.add_column("Content", style="white")
    table.add_column("Status", style="yellow")
    table.add_column("Score", style="red")
    table.add_column("Flags", style="magenta")
    table.add_column("Hidden", style="red")

    for row in result:
        # Parse flags
        flags = json.loads(row['moderation_flags']) if row['moderation_flags'] else []
        flags_str = ', '.join(flags) if flags else '-'

        # Truncate content
        content = row['content'][:50] + '...' if len(row['content']) > 50 else row['content']

        # Format score
        score = f"{row['moderation_score']:.2f}" if row['moderation_score'] else '0.00'

        # Status color
        status_display = row['moderation_status']

        table.add_row(
            row['id'][:12] + '...',
            row['actor_username'],
            content,
            status_display,
            score,
            flags_str,
            "Yes" if row['hidden'] else "No"
        )

    console.print(table)


@moderation.command()
@click.argument('reply_id')
@click.option('--remote', is_flag=True, help='Use remote database')
def approve(reply_id, remote):
    """Approve and unhide a reply."""
    query = f"""
        UPDATE replies
        SET moderation_status = 'approved', hidden = FALSE
        WHERE id LIKE '{reply_id}%'
    """

    execute_remote_d1(query, remote=remote)
    console.print(f"[green]✓[/green] Reply approved and unhidden: {reply_id}")


@moderation.command()
@click.argument('reply_id')
@click.option('--remote', is_flag=True, help='Use remote database')
def reject(reply_id, remote):
    """Reject and hide a reply permanently."""
    query = f"""
        UPDATE replies
        SET moderation_status = 'rejected', hidden = TRUE
        WHERE id LIKE '{reply_id}%'
    """

    execute_remote_d1(query, remote=remote)
    console.print(f"[red]✓[/red] Reply rejected and hidden: {reply_id}")


@moderation.command()
@click.argument('reply_id')
@click.option('--remote', is_flag=True, help='Use remote database')
def show(reply_id, remote):
    """Show full details of a moderated reply."""
    query = f"""
        SELECT r.id, r.post_id, r.actor_id, r.actor_username, r.actor_display_name,
               r.content, r.moderation_status, r.moderation_score, r.moderation_flags,
               r.moderation_checked_at, r.hidden, r.published_at,
               p.content as post_content
        FROM replies r
        LEFT JOIN posts p ON r.post_id = p.id
        WHERE r.id LIKE '{reply_id}%'
        LIMIT 1
    """

    result = execute_remote_d1(query, remote=remote)

    if not result:
        console.print(f"[red]Reply not found: {reply_id}[/red]")
        return

    reply = result[0]
    flags = json.loads(reply['moderation_flags']) if reply['moderation_flags'] else []

    # Display full reply details
    console.print(Panel(f"""
[bold]Reply ID:[/bold] {reply['id']}
[bold]Author:[/bold] {reply['actor_username']} ({reply['actor_display_name']})
[bold]Actor ID:[/bold] {reply['actor_id']}

[bold]Content:[/bold]
{reply['content']}

[bold]Reply to:[/bold] {reply['post_id']}
[bold]Original Post:[/bold]
{reply['post_content'][:100] + '...' if reply['post_content'] and len(reply['post_content']) > 100 else reply['post_content']}

[bold cyan]Moderation Details:[/bold cyan]
[bold]Status:[/bold] {reply['moderation_status']}
[bold]Score:[/bold] {reply['moderation_score']:.3f}
[bold]Flags:[/bold] {', '.join(flags) if flags else 'None'}
[bold]Hidden:[/bold] {'Yes' if reply['hidden'] else 'No'}
[bold]Checked At:[/bold] {reply['moderation_checked_at']}
[bold]Published At:[/bold] {reply['published_at']}
    """.strip(), title="Reply Details"))


@moderation.command()
@click.option('--remote', is_flag=True, help='Use remote database')
def settings(remote):
    """Show moderation settings."""
    query = """
        SELECT auto_hide_threshold, auto_reject_threshold, enabled,
               check_sentiment, check_toxicity, notify_on_flagged,
               updated_at
        FROM moderation_settings
        WHERE id = 1
    """

    result = execute_remote_d1(query, remote=remote)

    if not result:
        console.print("[red]Moderation settings not found[/red]")
        return

    settings = result[0]

    console.print(Panel(f"""
[bold]Moderation Enabled:[/bold] {'Yes' if settings['enabled'] else 'No'}

[bold cyan]Thresholds:[/bold cyan]
[bold]Auto-Hide Threshold:[/bold] {settings['auto_hide_threshold']} (scores above this are hidden)
[bold]Auto-Reject Threshold:[/bold] {settings['auto_reject_threshold']} (scores above this are rejected)

[bold cyan]Checks:[/bold cyan]
[bold]Check Sentiment:[/bold] {'Yes' if settings['check_sentiment'] else 'No'}
[bold]Check Toxicity:[/bold] {'Yes' if settings['check_toxicity'] else 'No'}
[bold]Notify on Flagged:[/bold] {'Yes' if settings['notify_on_flagged'] else 'No'}

[bold]Last Updated:[/bold] {settings['updated_at']}
    """.strip(), title="Moderation Settings"))


@moderation.command()
@click.option('--hide-threshold', type=float, help='Set auto-hide threshold (0.0-1.0)')
@click.option('--reject-threshold', type=float, help='Set auto-reject threshold (0.0-1.0)')
@click.option('--enabled/--disabled', default=None, help='Enable/disable moderation')
@click.option('--remote', is_flag=True, help='Use remote database')
def configure(hide_threshold, reject_threshold, enabled, remote):
    """Update moderation settings."""
    updates = []

    if hide_threshold is not None:
        if not 0.0 <= hide_threshold <= 1.0:
            console.print("[red]Error: hide-threshold must be between 0.0 and 1.0[/red]")
            return
        updates.append(f"auto_hide_threshold = {hide_threshold}")

    if reject_threshold is not None:
        if not 0.0 <= reject_threshold <= 1.0:
            console.print("[red]Error: reject-threshold must be between 0.0 and 1.0[/red]")
            return
        updates.append(f"auto_reject_threshold = {reject_threshold}")

    if enabled is not None:
        updates.append(f"enabled = {1 if enabled else 0}")

    if not updates:
        console.print("[yellow]No settings to update[/yellow]")
        return

    # Add timestamp
    updates.append("updated_at = CURRENT_TIMESTAMP")

    query = f"""
        UPDATE moderation_settings
        SET {', '.join(updates)}
        WHERE id = 1
    """

    execute_remote_d1(query, remote=remote)
    console.print("[green]✓[/green] Moderation settings updated")


@moderation.command()
@click.option('--remote', is_flag=True, help='Use remote database')
def stats(remote):
    """Show moderation statistics."""
    query = """
        SELECT
            COUNT(*) as total,
            SUM(CASE WHEN moderation_status = 'approved' THEN 1 ELSE 0 END) as approved,
            SUM(CASE WHEN moderation_status = 'pending' THEN 1 ELSE 0 END) as pending,
            SUM(CASE WHEN moderation_status = 'hidden' THEN 1 ELSE 0 END) as hidden,
            SUM(CASE WHEN moderation_status = 'rejected' THEN 1 ELSE 0 END) as rejected,
            SUM(CASE WHEN hidden = TRUE THEN 1 ELSE 0 END) as total_hidden,
            AVG(moderation_score) as avg_score,
            MAX(moderation_score) as max_score
        FROM replies
    """

    result = execute_remote_d1(query, remote=remote)

    if not result or result[0]['total'] == 0:
        console.print("[yellow]No replies to show statistics for[/yellow]")
        return

    stats = result[0]

    console.print(Panel(f"""
[bold cyan]Total Replies:[/bold cyan] {stats['total']}

[bold]Status Breakdown:[/bold]
  • Approved: {stats['approved']} ({stats['approved']/stats['total']*100:.1f}%)
  • Pending: {stats['pending']} ({stats['pending']/stats['total']*100:.1f}%)
  • Hidden: {stats['hidden']} ({stats['hidden']/stats['total']*100:.1f}%)
  • Rejected: {stats['rejected']} ({stats['rejected']/stats['total']*100:.1f}%)

[bold]Hidden Replies:[/bold] {stats['total_hidden']} ({stats['total_hidden']/stats['total']*100:.1f}%)

[bold]Score Statistics:[/bold]
  • Average Score: {stats['avg_score']:.3f}
  • Maximum Score: {stats['max_score']:.3f}
    """.strip(), title="Moderation Statistics"))
