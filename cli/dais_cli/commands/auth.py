"""Cloudflare Access authentication management commands."""

import click
import subprocess
import sys
import json
import secrets
from pathlib import Path
from rich.console import Console
from rich.table import Table
from rich.panel import Panel
from rich.markdown import Markdown

from dais_cli.config import Config

console = Console()


@click.group()
def auth():
    """Manage Cloudflare Access authentication."""
    pass


@auth.command()
def setup():
    """Set up Cloudflare Access authentication.

    This command guides you through configuring Cloudflare Access
    for your dais instance. You'll need:
    - A Cloudflare account
    - Cloudflare Zero Trust enabled
    - Your dais domain added to Cloudflare

    Example:
        dais auth setup
    """
    console.print(Panel.fit(
        "[bold cyan]Cloudflare Access Setup[/bold cyan]\n\n"
        "This will guide you through setting up authentication using\n"
        "Cloudflare Access (part of Cloudflare Zero Trust).",
        border_style="cyan"
    ))

    console.print("\n[bold]Step 1: Create a Cloudflare Zero Trust Account[/bold]")
    console.print("1. Go to https://one.dash.cloudflare.com/")
    console.print("2. Sign in with your Cloudflare account")
    console.print("3. Choose a team name (e.g., 'myteam')")
    console.print("4. Your team domain will be: [cyan]myteam.cloudflareaccess.com[/cyan]")

    team_domain = click.prompt("\nEnter your Cloudflare Access team domain", default="myteam.cloudflareaccess.com")

    console.print("\n[bold]Step 2: Create an Access Application[/bold]")
    console.print("1. Go to Access → Applications → Add an application")
    console.print("2. Choose 'Self-hosted'")
    console.print("3. Application name: 'dais API'")

    config = Config()
    config.load()
    domain = config.get("server.domain", "yourdomain.com")

    console.print(f"4. Application domain: [cyan]{domain}[/cyan]")
    console.print("5. Path: [cyan]/api/*[/cyan] (protect all API endpoints)")

    console.print("\n[bold]Step 3: Configure Identity Provider[/bold]")
    console.print("1. Choose an identity provider:")
    console.print("   - Google (recommended for personal use)")
    console.print("   - GitHub")
    console.print("   - Microsoft")
    console.print("   - One-time PIN (email)")
    console.print("2. Follow the prompts to connect your provider")

    console.print("\n[bold]Step 4: Create Access Policy[/bold]")
    console.print("1. Policy name: 'Allow my email'")
    console.print("2. Action: Allow")
    console.print("3. Configure rules:")

    email = click.prompt("   Enter your email address")

    console.print(f"4. Rule: Include → Emails → {email}")
    console.print("5. Click 'Save application'")

    console.print("\n[bold]Step 5: Get Application Audience (AUD) Tag[/bold]")
    console.print("1. Go to your Access application settings")
    console.print("2. Find 'Application Audience (AUD) Tag'")
    console.print("3. Copy the value (looks like a long hash)")

    aud_tag = click.prompt("\nEnter the Application AUD tag")

    # Upload secrets to Cloudflare Worker
    console.print("\n[bold]Step 6: Uploading configuration to Cloudflare...[/bold]")

    project_root = Path(__file__).parent.parent.parent.parent
    auth_worker_dir = project_root / "workers" / "auth"

    if not auth_worker_dir.exists():
        console.print("[red]Auth worker directory not found![/red]")
        sys.exit(1)

    secrets_to_set = [
        ("CLOUDFLARE_ACCESS_TEAM_DOMAIN", team_domain),
        ("CLOUDFLARE_ACCESS_AUD", aud_tag),
        ("ALLOWED_EMAIL", email),
    ]

    for secret_name, secret_value in secrets_to_set:
        console.print(f"[dim]Setting {secret_name}...[/dim]")
        try:
            subprocess.run(
                ['wrangler', 'secret', 'put', secret_name, '--env', 'production'],
                input=secret_value,
                text=True,
                check=True,
                cwd=str(auth_worker_dir),
                capture_output=True
            )
            console.print(f"[green]✓[/green] {secret_name} set")
        except subprocess.CalledProcessError as e:
            console.print(f"[red]✗ Failed to set {secret_name}: {e.stderr}[/red]")
            sys.exit(1)

    # Save to config for reference
    config.set("auth.team_domain", team_domain)
    config.set("auth.aud_tag", aud_tag)
    config.set("auth.allowed_email", email)
    config.save()

    console.print("\n[bold green]✓ Cloudflare Access configured successfully![/bold green]")

    console.print("\n[bold]Next Steps:[/bold]")
    console.print("1. Deploy the auth worker: [cyan]dais deploy workers[/cyan]")
    console.print("2. Test authentication: [cyan]dais auth test[/cyan]")
    console.print(f"3. Your apps can now authenticate at: [cyan]https://{domain}/api/auth/login[/cyan]")

    console.print("\n[dim]For mobile apps, use the authentication flow in AUTH_API.md[/dim]")


@auth.command()
@click.argument('name')
@click.option('--duration', default='8760h', help='Token duration (e.g., 8760h = 1 year)')
def create_service_token(name, duration):
    """Create a Cloudflare Access Service Token for API access.

    Service tokens are used for machine-to-machine authentication
    (automation, scripts, CI/CD) without requiring a browser login.

    NAME: Descriptive name for the service token

    Example:
        dais auth create-service-token "CI/CD Pipeline"
        dais auth create-service-token "Backup Script" --duration 2160h
    """
    console.print(f"[bold blue]Creating Cloudflare Access Service Token '{name}'...[/bold blue]\n")

    console.print("[yellow]Note: Service tokens must be created in the Cloudflare dashboard[/yellow]")
    console.print("\n[bold]Steps:[/bold]")
    console.print("1. Go to https://one.dash.cloudflare.com/")
    console.print("2. Navigate to: Access → Service Auth → Service Tokens")
    console.print("3. Click 'Create Service Token'")
    console.print(f"4. Name: [cyan]{name}[/cyan]")
    console.print(f"5. Duration: [cyan]{duration}[/cyan]")
    console.print("6. Click 'Generate token'")
    console.print("\n7. [bold yellow]Save the Client ID and Client Secret - they won't be shown again![/bold yellow]")

    if click.confirm("\nHave you created the service token?", default=True):
        client_id = click.prompt("Enter the Client ID")
        client_secret = click.prompt("Enter the Client Secret", hide_input=True)

        # Add to SERVICE_TOKENS environment variable
        project_root = Path(__file__).parent.parent.parent.parent
        auth_worker_dir = project_root / "workers" / "auth"

        # Get existing tokens
        try:
            result = subprocess.run(
                ['wrangler', 'secret', 'get', 'SERVICE_TOKENS', '--env', 'production'],
                capture_output=True,
                text=True,
                cwd=str(auth_worker_dir)
            )
            existing_tokens = json.loads(result.stdout) if result.stdout else []
        except:
            existing_tokens = []

        # Add new token
        existing_tokens.append({
            "name": name,
            "clientId": client_id,
            "clientSecret": client_secret
        })

        # Upload updated tokens
        tokens_json = json.dumps(existing_tokens)

        try:
            subprocess.run(
                ['wrangler', 'secret', 'put', 'SERVICE_TOKENS', '--env', 'production'],
                input=tokens_json,
                text=True,
                check=True,
                cwd=str(auth_worker_dir),
                capture_output=True
            )
            console.print(f"\n[green]✓[/green] Service token '[bold]{name}[/bold]' added successfully!")

            console.print("\n[bold]Using this token:[/bold]")
            console.print(f"  CF-Access-Client-Id: {client_id}")
            console.print(f"  CF-Access-Client-Secret: {client_secret}")

            console.print("\n[bold]Example usage with curl:[/bold]")
            config = Config()
            config.load()
            domain = config.get("server.domain", "yourdomain.com")

            console.print(f"""[dim]curl https://{domain}/api/posts \\
  -H "CF-Access-Client-Id: {client_id}" \\
  -H "CF-Access-Client-Secret: {client_secret}"[/dim]""")

        except subprocess.CalledProcessError as e:
            console.print(f"[red]✗ Failed to save service token: {e.stderr}[/red]")
            sys.exit(1)


@auth.command()
def list_service_tokens():
    """List configured service tokens.

    Shows all service tokens configured for API access.

    Example:
        dais auth list-service-tokens
    """
    project_root = Path(__file__).parent.parent.parent.parent
    auth_worker_dir = project_root / "workers" / "auth"

    try:
        result = subprocess.run(
            ['wrangler', 'secret', 'get', 'SERVICE_TOKENS', '--env', 'production'],
            capture_output=True,
            text=True,
            cwd=str(auth_worker_dir)
        )

        if not result.stdout:
            console.print("[yellow]No service tokens configured[/yellow]")
            console.print("\n[dim]Create one with: dais auth create-service-token \"Token Name\"[/dim]")
            return

        tokens = json.loads(result.stdout)

        if not tokens:
            console.print("[yellow]No service tokens configured[/yellow]")
            return

        table = Table(title="Cloudflare Access Service Tokens", show_header=True)
        table.add_column("Name", style="cyan")
        table.add_column("Client ID", style="dim")
        table.add_column("Secret", style="dim")

        for token in tokens:
            client_id = token.get('clientId', 'N/A')
            # Mask secret for security
            secret_masked = token.get('clientSecret', 'N/A')[:8] + '...' if token.get('clientSecret') else 'N/A'

            table.add_row(
                token.get('name', 'Unknown'),
                client_id,
                secret_masked
            )

        console.print(table)

    except subprocess.CalledProcessError:
        console.print("[yellow]Could not retrieve service tokens[/yellow]")
    except json.JSONDecodeError:
        console.print("[red]Invalid SERVICE_TOKENS format[/red]")


@auth.command()
def test():
    """Test Cloudflare Access authentication.

    Verifies that Cloudflare Access is properly configured and working.

    Example:
        dais auth test
    """
    import httpx

    config = Config()
    config.load()
    domain = config.get("server.domain", "yourdomain.com")

    console.print("[bold blue]Testing Cloudflare Access Authentication...[/bold blue]\n")

    # Test 1: Check auth worker health
    console.print("[bold]1. Testing auth worker health endpoint...[/bold]")
    try:
        response = httpx.get(f"https://{domain}/api/auth/health", timeout=10.0, follow_redirects=False)

        if response.status_code == 302:
            console.print("[yellow]⚠[/yellow] Redirected to Cloudflare Access login (expected)")
            console.print("[green]✓[/green] Cloudflare Access is protecting the endpoint")
        elif response.status_code == 200:
            data = response.json()
            console.print(f"[green]✓[/green] Auth worker is running")
            console.print(f"[dim]Team: {data.get('team_domain', 'N/A')}[/dim]")
        else:
            console.print(f"[red]✗[/red] Unexpected status: {response.status_code}")

    except Exception as e:
        console.print(f"[red]✗[/red] Health check failed: {e}")

    # Test 2: Check login endpoint
    console.print("\n[bold]2. Testing login endpoint...[/bold]")
    try:
        response = httpx.get(f"https://{domain}/api/auth/login", timeout=10.0, follow_redirects=False)

        if response.status_code == 302:
            console.print("[yellow]⚠[/yellow] Redirected to Cloudflare Access (expected for browsers)")
            console.print("[green]✓[/green] Authentication is enabled")
        elif response.status_code == 200:
            data = response.json()
            console.print(f"[green]✓[/green] Login endpoint accessible")
            console.print(f"[dim]Login URL: {data.get('login_url', 'N/A')}[/dim]")
        else:
            console.print(f"[yellow]⚠[/yellow] Status: {response.status_code}")

    except Exception as e:
        console.print(f"[red]✗[/red] Login test failed: {e}")

    console.print("\n[bold]Summary:[/bold]")
    console.print("If you see redirects to Cloudflare Access, that's correct!")
    console.print("Your API is protected and ready for authentication.")

    console.print("\n[bold]Next Steps:[/bold]")
    console.print("1. Access your API through a browser to test login")
    console.print(f"2. Visit: [cyan]https://{domain}/api/auth/login[/cyan]")
    console.print("3. You should be redirected to Cloudflare Access login")
    console.print("4. After login, you can access protected endpoints")


@auth.command()
def status():
    """Show Cloudflare Access authentication status.

    Displays current authentication configuration.

    Example:
        dais auth status
    """
    config = Config()
    config.load()

    console.print("[bold blue]Cloudflare Access Status[/bold blue]\n")

    console.print("[bold]Configuration:[/bold]")

    team_domain = config.get("auth.team_domain")
    if team_domain:
        console.print(f"  Team Domain: [cyan]{team_domain}[/cyan]")
    else:
        console.print("  Team Domain: [yellow]Not configured[/yellow]")

    allowed_email = config.get("auth.allowed_email")
    if allowed_email:
        console.print(f"  Allowed Email: [cyan]{allowed_email}[/cyan]")
    else:
        console.print("  Allowed Email: [yellow]Not configured[/yellow]")

    domain = config.get("server.domain", "yourdomain.com")
    console.print(f"  API Domain: [cyan]{domain}[/cyan]")
    console.print(f"  Auth Endpoint: [dim]https://{domain}/api/auth/login[/dim]")

    if not team_domain:
        console.print("\n[yellow]⚠ Cloudflare Access not configured[/yellow]")
        console.print("[dim]Run 'dais auth setup' to configure authentication[/dim]")
    else:
        console.print("\n[green]✓ Cloudflare Access is configured[/green]")
        console.print(f"\n[dim]Dashboard: https://one.dash.cloudflare.com/[/dim]")

    console.print("\n[bold]Available Commands:[/bold]")
    console.print("  [cyan]dais auth setup[/cyan]                  - Configure Cloudflare Access")
    console.print("  [cyan]dais auth test[/cyan]                   - Test authentication")
    console.print("  [cyan]dais auth create-service-token[/cyan]  - Create API token")
    console.print("  [cyan]dais auth list-service-tokens[/cyan]   - List API tokens")


@auth.command()
def docs():
    """Show Cloudflare Access documentation links.

    Provides helpful links to Cloudflare Access documentation.

    Example:
        dais auth docs
    """
    console.print("[bold blue]Cloudflare Access Documentation[/bold blue]\n")

    console.print("[bold]Getting Started:[/bold]")
    console.print("  https://developers.cloudflare.com/cloudflare-one/applications/")

    console.print("\n[bold]Identity Providers:[/bold]")
    console.print("  https://developers.cloudflare.com/cloudflare-one/identity/idp-integration/")

    console.print("\n[bold]Service Tokens (API Access):[/bold]")
    console.print("  https://developers.cloudflare.com/cloudflare-one/identity/service-tokens/")

    console.print("\n[bold]JWT Verification:[/bold]")
    console.print("  https://developers.cloudflare.com/cloudflare-one/identity/authorization-cookie/validating-json/")

    console.print("\n[bold]Access Policies:[/bold]")
    console.print("  https://developers.cloudflare.com/cloudflare-one/policies/access/")

    console.print("\n[bold]Mobile App Integration:[/bold]")
    console.print("  See AUTH_API.md in the dais repository")
