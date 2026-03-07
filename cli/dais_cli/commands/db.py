"""Database management commands."""

import click
from rich.console import Console
from rich.table import Table
import subprocess
import sys
from pathlib import Path

console = Console()


@click.group()
def db():
    """Manage D1 database."""
    pass


@db.command()
@click.option('--remote', is_flag=True, help='Execute on remote database')
def migrate(remote):
    """Run database migrations.

    Applies all pending SQL migrations from cli/migrations/ to the D1 database.
    """
    migrations_dir = Path(__file__).parent.parent.parent / "migrations"

    if not migrations_dir.exists():
        console.print("[red]Migrations directory not found[/red]")
        return

    # Get all .sql files sorted
    migrations = sorted(migrations_dir.glob("*.sql"))

    if not migrations:
        console.print("[yellow]No migrations found[/yellow]")
        return

    console.print(f"[bold blue]Found {len(migrations)} migration(s)[/bold blue]\n")

    for migration in migrations:
        console.print(f"[dim]Applying {migration.name}...[/dim]")

        cmd = ["wrangler", "d1", "execute", "dais-social", "--file", str(migration)]
        if remote:
            cmd.append("--remote")

        try:
            result = subprocess.run(cmd, capture_output=True, text=True, check=True)
            console.print(f"[green]✓[/green] {migration.name} applied")
        except subprocess.CalledProcessError as e:
            console.print(f"[red]✗ Failed to apply {migration.name}[/red]")
            console.print(f"[red]{e.stderr}[/red]")
            sys.exit(1)

    console.print("\n[bold green]All migrations applied successfully![/bold green]")


@db.command()
@click.option('--remote', is_flag=True, help='Query remote database')
def tables(remote):
    """List all tables in the database."""
    cmd = [
        "wrangler", "d1", "execute", "dais-social",
        "--command", "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
    ]
    if remote:
        cmd.append("--remote")

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        console.print("[bold blue]Database Tables[/bold blue]\n")

        # Parse the output - wrangler returns JSON
        import json
        output = result.stdout

        # Find the JSON array in the output
        if '"results"' in output:
            # Extract just the results portion
            start = output.find('[')
            end = output.rfind(']') + 1
            data = json.loads(output[start:end])

            table = Table(show_header=True, header_style="bold cyan")
            table.add_column("Table Name", style="green")

            for item in data[0].get("results", []):
                if item.get("name") and not item["name"].startswith("_cf_"):
                    table.add_row(item["name"])

            console.print(table)
        else:
            console.print(output)

    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Error querying database[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)


@db.command()
@click.argument('sql')
@click.option('--remote', is_flag=True, help='Execute on remote database')
def query(sql, remote):
    """Execute a SQL query.

    SQL: The SQL query to execute
    """
    cmd = ["wrangler", "d1", "execute", "dais-social", "--command", sql]
    if remote:
        cmd.append("--remote")

    console.print(f"[dim]Executing: {sql}[/dim]\n")

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        console.print(result.stdout)
    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Query failed[/red]")
        console.print(f"[red]{e.stderr}[/red]")
        sys.exit(1)


@db.command()
@click.option('--remote', is_flag=True, help='Query remote database')
def info(remote):
    """Show database information and statistics."""
    queries = [
        ("Actors", "SELECT COUNT(*) as count FROM actors"),
        ("Followers", "SELECT COUNT(*) as count FROM followers"),
        ("Posts", "SELECT COUNT(*) as count FROM posts"),
        ("Activities", "SELECT COUNT(*) as count FROM activities"),
    ]

    console.print("[bold blue]Database Information[/bold blue]\n")

    table = Table(show_header=True, header_style="bold cyan")
    table.add_column("Table", style="green")
    table.add_column("Count", justify="right", style="yellow")

    for name, query in queries:
        cmd = ["wrangler", "d1", "execute", "dais-social", "--command", query]
        if remote:
            cmd.append("--remote")

        try:
            result = subprocess.run(cmd, capture_output=True, text=True, check=True)
            # Parse count from output
            import json
            output = result.stdout
            start = output.find('[')
            end = output.rfind(']') + 1
            data = json.loads(output[start:end])
            count = data[0]["results"][0]["count"]
            table.add_row(name, str(count))
        except Exception as e:
            table.add_row(name, "[red]error[/red]")

    console.print(table)

    # Show database location
    location = "Remote" if remote else "Local"
    console.print(f"\n[dim]Database: dais-social ({location})[/dim]")
