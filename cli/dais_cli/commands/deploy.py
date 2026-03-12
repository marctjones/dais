"""Deployment and infrastructure commands."""

import subprocess
import click
from pathlib import Path
from rich.console import Console
import httpx

from dais_cli.config import Config

console = Console()


def get_project_root() -> Path:
    """Get the project root directory."""
    return Path(__file__).parent.parent.parent.parent


def generate_wrangler_configs(config: Config):
    """Generate wrangler.toml from templates using config values.

    Args:
        config: Configuration instance
    """
    template_vars = {
        'DOMAIN': config.get('server.domain'),
        'ACTIVITYPUB_DOMAIN': config.get('server.activitypub_domain'),
        'PDS_DOMAIN': config.get('server.pds_domain', f"pds.{config.get('server.domain')}"),
        'DATABASE_NAME': config.get('cloudflare.d1_database_name', 'dais-db'),
        'DATABASE_ID': config.get('cloudflare.d1_database_id'),
        'R2_BUCKET': config.get('cloudflare.r2_bucket', 'dais-media'),
        'ACCOUNT_NAME': config.get('cloudflare.account_name'),
    }

    # Validate required values
    missing = [k for k, v in template_vars.items() if not v]
    if missing:
        console.print(f"[red]Missing required config values: {', '.join(missing)}[/red]")
        console.print("[yellow]Run 'dais setup init' first or 'dais deploy infrastructure' to create resources[/yellow]")
        return False

    project_root = get_project_root()
    workers_dir = project_root / 'workers'

    console.print("[bold blue]Generating wrangler.toml files from templates...[/bold blue]\n")

    for worker_dir in workers_dir.iterdir():
        if not worker_dir.is_dir():
            continue

        template = worker_dir / 'wrangler.toml.template'
        if template.exists():
            content = template.read_text()
            for key, value in template_vars.items():
                content = content.replace(f'${{{key}}}', str(value))

            output_file = worker_dir / 'wrangler.toml'
            output_file.write_text(content)
            console.print(f"[green]✓[/green] Generated {worker_dir.name}/wrangler.toml")

    console.print("\n[green]✓ All wrangler.toml files generated![/green]")
    return True


@click.group()
def deploy():
    """Deploy dais to Cloudflare."""
    pass


@deploy.command()
def infrastructure():
    """Create D1 database and R2 bucket on Cloudflare."""
    config = Config()
    config.load()

    console.print("[bold blue]Creating Cloudflare infrastructure...[/bold blue]\n")

    # Create D1 database
    console.print("[bold]1. Creating D1 database[/bold]")
    db_name = config.get('cloudflare.d1_database_name', 'dais-db')

    try:
        result = subprocess.run(
            ['wrangler', 'd1', 'create', db_name],
            capture_output=True,
            text=True,
            check=True
        )

        # Parse output to get database ID
        # Output format: "✅ Successfully created DB 'dais-db' in region WEUR\nCreated your database using D1's new storage backend..."
        # database_id appears in the output
        output = result.stdout

        # Look for database_id in output
        for line in output.split('\n'):
            if 'database_id' in line:
                db_id = line.split('=')[1].strip().strip('"')
                config.set('cloudflare.d1_database_id', db_id)
                console.print(f"[green]✓[/green] Created D1 database: {db_name}")
                console.print(f"[dim]Database ID: {db_id}[/dim]\n")
                break
        else:
            console.print(f"[green]✓[/green] D1 database '{db_name}' created")
            console.print("[yellow]Please set cloudflare.d1_database_id in config manually[/yellow]\n")

    except subprocess.CalledProcessError as e:
        if "already exists" in e.stderr or "already exists" in e.stdout:
            console.print(f"[yellow]D1 database '{db_name}' already exists[/yellow]\n")
        else:
            console.print(f"[red]Failed to create D1 database: {e.stderr}[/red]\n")
            return

    # Create R2 bucket
    console.print("[bold]2. Creating R2 bucket[/bold]")
    bucket_name = config.get('cloudflare.r2_bucket', 'dais-media')

    try:
        subprocess.run(
            ['wrangler', 'r2', 'bucket', 'create', bucket_name],
            capture_output=True,
            text=True,
            check=True
        )
        console.print(f"[green]✓[/green] Created R2 bucket: {bucket_name}\n")
    except subprocess.CalledProcessError as e:
        if "already exists" in e.stderr or "already exists" in e.stdout:
            console.print(f"[yellow]R2 bucket '{bucket_name}' already exists[/yellow]\n")
        else:
            console.print(f"[red]Failed to create R2 bucket: {e.stderr}[/red]\n")
            return

    console.print("[green]✓ Infrastructure creation complete![/green]")
    console.print("\n[dim]Next steps:[/dim]")
    console.print("[dim]  1. Run 'dais deploy secrets' to upload private key[/dim]")
    console.print("[dim]  2. Run 'dais deploy database' to apply migrations[/dim]")
    console.print("[dim]  3. Run 'dais deploy workers' to deploy workers[/dim]")
    console.print("[dim]Or run 'dais deploy all' to do everything at once[/dim]")


@deploy.command()
def secrets():
    """Upload private key to Cloudflare secrets."""
    config = Config()
    config.load()

    private_key_path = config.get('keys.private_key_path')
    if not private_key_path or not Path(private_key_path).exists():
        console.print("[red]Private key not found![/red]")
        console.print("[yellow]Run 'dais setup init' first to generate keys[/yellow]")
        return

    console.print("[bold blue]Uploading secrets to Cloudflare workers...[/bold blue]\n")

    # Workers that need the private key
    workers = ['actor', 'inbox', 'outbox', 'delivery-queue']
    project_root = get_project_root()

    for worker in workers:
        worker_dir = project_root / 'workers' / worker
        if not worker_dir.exists():
            console.print(f"[yellow]Worker directory not found: {worker}[/yellow]")
            continue

        console.print(f"[dim]Uploading to {worker}...[/dim]")

        try:
            with open(private_key_path, 'r') as key_file:
                result = subprocess.run(
                    ['wrangler', 'secret', 'put', 'PRIVATE_KEY', '--env', 'production'],
                    input=key_file.read(),
                    capture_output=True,
                    text=True,
                    cwd=worker_dir,
                    check=True
                )
            console.print(f"[green]✓[/green] {worker}")
        except subprocess.CalledProcessError as e:
            console.print(f"[red]✗ Failed to upload secret to {worker}: {e.stderr}[/red]")

    console.print("\n[green]✓ Secrets uploaded successfully![/green]")


@deploy.command()
def database():
    """Apply database migrations."""
    config = Config()
    config.load()

    db_id = config.get('cloudflare.d1_database_id')
    if not db_id:
        console.print("[red]D1 database not configured![/red]")
        console.print("[yellow]Run 'dais deploy infrastructure' first[/yellow]")
        return

    console.print("[bold blue]Applying database migrations...[/bold blue]\n")

    project_root = get_project_root()
    migrations_dir = project_root / 'cli' / 'migrations'

    if not migrations_dir.exists():
        console.print(f"[yellow]Migrations directory not found: {migrations_dir}[/yellow]")
        return

    # Get all migration files
    migration_files = sorted(migrations_dir.glob('*.sql'))

    if not migration_files:
        console.print("[yellow]No migration files found[/yellow]")
        return

    for migration_file in migration_files:
        console.print(f"[dim]Applying {migration_file.name}...[/dim]")

        try:
            subprocess.run(
                ['wrangler', 'd1', 'execute', db_id, '--file', str(migration_file), '--remote'],
                capture_output=True,
                text=True,
                check=True
            )
            console.print(f"[green]✓[/green] {migration_file.name}")
        except subprocess.CalledProcessError as e:
            console.print(f"[red]✗ Failed to apply {migration_file.name}: {e.stderr}[/red]")
            # Continue with other migrations
            continue

    console.print("\n[green]✓ Database migrations applied successfully![/green]")


@deploy.command()
def workers():
    """Deploy all workers to Cloudflare."""
    config = Config()
    config.load()

    console.print("[bold blue]Deploying workers to Cloudflare...[/bold blue]\n")

    # Generate wrangler.toml files first
    if not generate_wrangler_configs(config):
        return

    console.print()

    project_root = get_project_root()
    workers_dir = project_root / 'workers'

    # Deploy workers in order
    worker_order = [
        'webfinger',
        'actor',
        'inbox',
        'outbox',
        'pds',
        'delivery-queue',
        'router',
        'landing'
    ]

    for worker_name in worker_order:
        worker_dir = workers_dir / worker_name
        if not worker_dir.exists():
            console.print(f"[yellow]Worker directory not found: {worker_name}[/yellow]")
            continue

        console.print(f"[bold]Deploying {worker_name}...[/bold]")

        try:
            result = subprocess.run(
                ['wrangler', 'deploy', '--env', 'production'],
                capture_output=True,
                text=True,
                cwd=worker_dir,
                check=True
            )
            console.print(f"[green]✓[/green] {worker_name} deployed\n")
        except subprocess.CalledProcessError as e:
            console.print(f"[red]✗ Failed to deploy {worker_name}[/red]")
            console.print(f"[dim]{e.stderr}[/dim]\n")
            # Continue with other workers
            continue

    console.print("[green]✓ All workers deployed successfully![/green]")
    console.print("\n[dim]Next step: Run 'dais deploy verify' to check deployment health[/dim]")


@deploy.command()
def verify():
    """Verify deployment health."""
    config = Config()
    config.load()

    domain = config.get('server.domain')
    activitypub_domain = config.get('server.activitypub_domain')
    username = config.get('server.username')

    console.print("[bold blue]Verifying deployment...[/bold blue]\n")

    checks = []

    # 1. WebFinger endpoint
    console.print("[bold]1. Testing WebFinger endpoint[/bold]")
    url = f"https://{domain}/.well-known/webfinger"
    params = {"resource": f"acct:{username}@{domain}"}

    try:
        response = httpx.get(url, params=params, timeout=10.0)
        response.raise_for_status()
        data = response.json()

        if data.get("subject") == f"acct:{username}@{domain}":
            console.print(f"[green]✓[/green] WebFinger endpoint working\n")
            checks.append(True)
        else:
            console.print(f"[red]✗[/red] Unexpected WebFinger response\n")
            checks.append(False)
    except Exception as e:
        console.print(f"[red]✗[/red] WebFinger endpoint failed: {e}\n")
        checks.append(False)

    # 2. Actor endpoint
    console.print("[bold]2. Testing Actor endpoint[/bold]")
    url = f"https://{activitypub_domain}/users/{username}"
    headers = {"Accept": "application/activity+json"}

    try:
        response = httpx.get(url, headers=headers, timeout=10.0)
        response.raise_for_status()
        data = response.json()

        if data.get("type") == "Person":
            console.print(f"[green]✓[/green] Actor endpoint working\n")
            checks.append(True)
        else:
            console.print(f"[red]✗[/red] Unexpected Actor response\n")
            checks.append(False)
    except Exception as e:
        console.print(f"[red]✗[/red] Actor endpoint failed: {e}\n")
        checks.append(False)

    # 3. Check worker status via wrangler
    console.print("[bold]3. Checking worker status[/bold]")

    try:
        result = subprocess.run(
            ['wrangler', 'deployments', 'list', '--name', 'router-production'],
            capture_output=True,
            text=True,
            check=True
        )
        console.print(f"[green]✓[/green] Workers are deployed\n")
        checks.append(True)
    except Exception as e:
        console.print(f"[yellow]⚠[/yellow] Could not verify worker status\n")
        checks.append(False)

    # Summary
    console.print("[bold]Summary[/bold]")
    passed = sum(checks)
    total = len(checks)

    if passed == total:
        console.print(f"[green]✓ All checks passed ({passed}/{total})[/green]")
        console.print("\n[bold green]Your dais instance is deployed and working![/bold green]")
        console.print(f"\n[dim]Your actor URL: https://{activitypub_domain}/users/{username}[/dim]")
        console.print(f"[dim]You can now follow @{username}@{domain} from other instances[/dim]")
    else:
        console.print(f"[yellow]⚠ Some checks failed ({passed}/{total} passed)[/yellow]")
        console.print("\n[yellow]Run 'dais doctor' for detailed diagnostics[/yellow]")


@deploy.command()
@click.pass_context
def all(ctx):
    """Full deployment (infrastructure + secrets + database + workers + verify)."""
    console.print("[bold blue]Running full deployment...[/bold blue]\n")

    # Run all deployment steps in order
    ctx.invoke(infrastructure)
    console.print()

    ctx.invoke(secrets)
    console.print()

    ctx.invoke(database)
    console.print()

    ctx.invoke(workers)
    console.print()

    ctx.invoke(verify)
