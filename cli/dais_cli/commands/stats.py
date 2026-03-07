"""Statistics and reporting commands."""

import click
from rich.console import Console
from rich.panel import Panel

console = Console()


@click.command()
def stats():
    """Show server statistics."""
    console.print("[bold blue]dais.social Statistics[/bold blue]\n")

    # TODO: Query D1 database for stats
    # - Follower count
    # - Following count
    # - Post count
    # - Recent activity

    console.print(Panel(
        """[bold]Followers:[/bold] 0
[bold]Following:[/bold] 0
[bold]Posts:[/bold] 0
[bold]Activities (30d):[/bold] 0

[dim]Statistics will be populated once the server is running[/dim]
        """,
        title="Server Statistics",
        border_style="blue"
    ))
