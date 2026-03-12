"""Diagnostic commands for troubleshooting dais setup."""

import subprocess
import click
from pathlib import Path
from rich.console import Console
from rich.table import Table
import httpx

from dais_cli.config import Config

console = Console()


def check_config_exists() -> tuple[bool, str]:
    """Check if config file exists."""
    config = Config()
    if config.config_file.exists():
        return True, f"Config found at {config.config_file}"
    return False, f"Config not found (expected at {config.config_file})"


def check_keys_exist() -> tuple[bool, str]:
    """Check if cryptographic keys exist."""
    config = Config()
    config.load()

    private_key = config.get('keys.private_key_path')
    public_key = config.get('keys.public_key_path')

    if private_key and Path(private_key).exists() and public_key and Path(public_key).exists():
        return True, "Private and public keys found"
    return False, "Keys missing or not configured"


def check_wrangler() -> tuple[bool, str]:
    """Check if wrangler is installed."""
    try:
        result = subprocess.run(
            ['wrangler', '--version'],
            capture_output=True,
            text=True,
            timeout=5
        )
        if result.returncode == 0:
            version = result.stdout.strip()
            return True, f"wrangler installed ({version})"
        return False, "wrangler not working properly"
    except FileNotFoundError:
        return False, "wrangler not installed"
    except subprocess.TimeoutExpired:
        return False, "wrangler command timed out"
    except Exception as e:
        return False, f"wrangler check failed: {e}"


def check_cf_auth() -> tuple[bool, str]:
    """Check if Cloudflare is authenticated."""
    try:
        result = subprocess.run(
            ['wrangler', 'whoami'],
            capture_output=True,
            text=True,
            timeout=10
        )
        if result.returncode == 0 and "You are logged in" in result.stdout:
            # Extract account info from output
            lines = result.stdout.split('\n')
            for line in lines:
                if 'Account Name:' in line or 'Account ID:' in line:
                    return True, "Cloudflare authenticated"
            return True, "Cloudflare authenticated"
        return False, "Not logged in to Cloudflare"
    except Exception as e:
        return False, f"Auth check failed: {e}"


def check_d1_exists() -> tuple[bool, str]:
    """Check if D1 database exists."""
    config = Config()
    config.load()

    db_id = config.get('cloudflare.d1_database_id')
    if not db_id:
        return False, "D1 database not configured"

    try:
        result = subprocess.run(
            ['wrangler', 'd1', 'list'],
            capture_output=True,
            text=True,
            timeout=10
        )
        if result.returncode == 0 and db_id in result.stdout:
            return True, f"D1 database exists (ID: {db_id[:8]}...)"
        return False, "D1 database not found"
    except Exception as e:
        return False, f"D1 check failed: {e}"


def check_r2_exists() -> tuple[bool, str]:
    """Check if R2 bucket exists."""
    config = Config()
    config.load()

    bucket = config.get('cloudflare.r2_bucket', 'dais-media')

    try:
        result = subprocess.run(
            ['wrangler', 'r2', 'bucket', 'list'],
            capture_output=True,
            text=True,
            timeout=10
        )
        if result.returncode == 0 and bucket in result.stdout:
            return True, f"R2 bucket exists ({bucket})"
        return False, "R2 bucket not found"
    except Exception as e:
        return False, f"R2 check failed: {e}"


def check_workers_deployed() -> tuple[bool, str]:
    """Check if workers are deployed."""
    try:
        result = subprocess.run(
            ['wrangler', 'deployments', 'list', '--name', 'router-production'],
            capture_output=True,
            text=True,
            timeout=10
        )
        if result.returncode == 0:
            return True, "Workers deployed"
        return False, "Workers not deployed"
    except Exception as e:
        return False, f"Worker check failed: {e}"


def check_webfinger() -> tuple[bool, str]:
    """Check if WebFinger endpoint responds."""
    config = Config()
    config.load()

    domain = config.get('server.domain')
    username = config.get('server.username')

    if not domain or domain == 'example.com':
        return False, "Domain not configured"

    try:
        url = f"https://{domain}/.well-known/webfinger"
        params = {"resource": f"acct:{username}@{domain}"}
        response = httpx.get(url, params=params, timeout=10.0)
        response.raise_for_status()

        data = response.json()
        if data.get("subject") == f"acct:{username}@{domain}":
            return True, "WebFinger endpoint working"
        return False, "WebFinger response invalid"
    except httpx.HTTPStatusError as e:
        return False, f"WebFinger HTTP {e.response.status_code}"
    except httpx.RequestError:
        return False, "WebFinger endpoint unreachable"
    except Exception as e:
        return False, f"WebFinger check failed: {e}"


def check_actor() -> tuple[bool, str]:
    """Check if Actor endpoint responds."""
    config = Config()
    config.load()

    activitypub_domain = config.get('server.activitypub_domain')
    username = config.get('server.username')

    if not activitypub_domain or activitypub_domain == 'social.example.com':
        return False, "ActivityPub domain not configured"

    try:
        url = f"https://{activitypub_domain}/users/{username}"
        headers = {"Accept": "application/activity+json"}
        response = httpx.get(url, headers=headers, timeout=10.0)
        response.raise_for_status()

        data = response.json()
        if data.get("type") == "Person":
            return True, "Actor endpoint working"
        return False, "Actor response invalid"
    except httpx.HTTPStatusError as e:
        return False, f"Actor HTTP {e.response.status_code}"
    except httpx.RequestError:
        return False, "Actor endpoint unreachable"
    except Exception as e:
        return False, f"Actor check failed: {e}"


@click.command()
def doctor():
    """Check dais setup and diagnose issues."""
    console.print("[bold blue]Running dais diagnostics...[/bold blue]\n")

    checks = [
        ("Config file exists", check_config_exists),
        ("Keys generated", check_keys_exist),
        ("Wrangler installed", check_wrangler),
        ("Cloudflare authenticated", check_cf_auth),
        ("D1 database exists", check_d1_exists),
        ("R2 bucket exists", check_r2_exists),
        ("Workers deployed", check_workers_deployed),
        ("WebFinger responding", check_webfinger),
        ("Actor responding", check_actor),
    ]

    results = []

    for name, check_fn in checks:
        console.print(f"[dim]Checking {name}...[/dim]", end="\r")
        success, message = check_fn()
        results.append((name, success, message))

    # Clear the checking line
    console.print(" " * 50, end="\r")

    # Create results table
    table = Table(show_header=True, header_style="bold")
    table.add_column("Check", style="cyan")
    table.add_column("Status", justify="center")
    table.add_column("Details", style="dim")

    for name, success, message in results:
        status = "[green]✓[/green]" if success else "[red]✗[/red]"
        table.add_row(name, status, message)

    console.print(table)

    # Summary
    passed = sum(1 for _, success, _ in results if success)
    total = len(results)

    console.print()

    if passed == total:
        console.print(f"[bold green]✓ All checks passed ({passed}/{total})[/bold green]")
        console.print("\n[dim]Your dais instance is healthy![/dim]")
    else:
        console.print(f"[bold yellow]⚠ {total - passed} issue(s) found ({passed}/{total} passed)[/bold yellow]")

        # Provide helpful suggestions based on failures
        console.print("\n[bold]Suggestions:[/bold]")

        for name, success, _ in results:
            if not success:
                if "Config file" in name:
                    console.print("[yellow]•[/yellow] Run 'dais setup init' to create configuration")
                elif "Keys" in name:
                    console.print("[yellow]•[/yellow] Run 'dais setup init' to generate cryptographic keys")
                elif "Wrangler" in name:
                    console.print("[yellow]•[/yellow] Install wrangler: npm install -g wrangler")
                elif "Cloudflare authenticated" in name:
                    console.print("[yellow]•[/yellow] Run 'wrangler login' to authenticate with Cloudflare")
                elif "D1 database" in name:
                    console.print("[yellow]•[/yellow] Run 'dais deploy infrastructure' to create D1 database")
                elif "R2 bucket" in name:
                    console.print("[yellow]•[/yellow] Run 'dais deploy infrastructure' to create R2 bucket")
                elif "Workers deployed" in name:
                    console.print("[yellow]•[/yellow] Run 'dais deploy workers' to deploy workers")
                elif "WebFinger" in name or "Actor" in name:
                    console.print("[yellow]•[/yellow] Check DNS configuration and worker deployment")
