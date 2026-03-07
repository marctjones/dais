"""Statistics and reporting commands."""

import click
from rich.console import Console
from rich.panel import Panel
from rich.table import Table
import subprocess
import json
import sys
from pathlib import Path

console = Console()


def query_d1(query: str, remote: bool = False) -> list:
    """Execute a D1 query and return results."""
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

        return data[0].get("results", [])
    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Error querying database: {e.stderr}[/red]")
        sys.exit(1)
    except json.JSONDecodeError as e:
        console.print(f"[red]✗ Error parsing database response: {e}[/red]")
        sys.exit(1)


@click.command()
@click.option('--remote', is_flag=True, help='Query remote database')
def stats(remote):
    """Show server statistics."""
    console.print("[bold blue]dais.social Statistics[/bold blue]\n")

    # Query follower counts by status
    follower_stats_query = "SELECT status, COUNT(*) as count FROM followers GROUP BY status"
    follower_stats = query_d1(follower_stats_query, remote)

    # Query total post count
    post_count_query = "SELECT COUNT(*) as count FROM posts"
    post_count_result = query_d1(post_count_query, remote)
    post_count = post_count_result[0]["count"] if post_count_result else 0

    # Query total activity count
    activity_count_query = "SELECT COUNT(*) as count FROM activities"
    activity_count_result = query_d1(activity_count_query, remote)
    activity_count = activity_count_result[0]["count"] if activity_count_result else 0

    # Calculate follower counts
    approved_count = 0
    pending_count = 0
    total_followers = 0

    for stat in follower_stats:
        count = stat["count"]
        total_followers += count
        if stat["status"] == "approved":
            approved_count = count
        elif stat["status"] == "pending":
            pending_count = count

    # Display statistics
    console.print(Panel(
        f"""[bold]Total Followers:[/bold] {total_followers}
  [green]✓ Approved:[/green] {approved_count}
  [yellow]⏳ Pending:[/yellow] {pending_count}

[bold]Posts:[/bold] {post_count}
[bold]Activities:[/bold] {activity_count}

[dim]Database: {'remote (production)' if remote else 'local (development)'}[/dim]
        """,
        title="Server Statistics",
        border_style="blue"
    ))

    # Show follower breakdown table if there are followers
    if follower_stats:
        console.print("\n[bold]Follower Breakdown:[/bold]")
        table = Table(show_header=True, header_style="bold cyan")
        table.add_column("Status", style="cyan")
        table.add_column("Count", style="green", justify="right")

        for stat in follower_stats:
            status_icon = {
                "approved": "✓",
                "pending": "⏳",
                "rejected": "✗"
            }.get(stat["status"], "")
            table.add_row(f"{status_icon} {stat['status']}", str(stat["count"]))

        console.print(table)
