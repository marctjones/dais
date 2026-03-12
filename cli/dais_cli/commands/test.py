"""Testing and validation commands."""

import click
from rich.console import Console
import httpx

from dais_cli.config import Config

console = Console()


@click.group()
def test():
    """Test endpoints and federation."""
    pass


@test.command()
@click.option('--local', is_flag=True, help='Test local development server')
def webfinger(local):
    """Test WebFinger endpoint."""
    config = Config()
    config.load()

    if local:
        domain = "localhost:8787"
        protocol = "http"
        username = config.get("server.username", "marc")
    else:
        domain = config.get("server.domain", "dais.social")
        protocol = "https"
        username = config.get("server.username", "marc")

    console.print(f"[bold blue]Testing WebFinger endpoint...[/bold blue]\n")

    url = f"{protocol}://{domain}/.well-known/webfinger"
    params = {"resource": f"acct:{username}@{domain}" if not local else f"acct:{username}@dais.social"}

    console.print(f"[dim]GET {url}?resource={params['resource']}[/dim]\n")

    try:
        response = httpx.get(url, params=params, timeout=10.0)
        response.raise_for_status()

        console.print(f"[green]✓[/green] Status: {response.status_code}")
        console.print(f"[green]✓[/green] Content-Type: {response.headers.get('content-type')}\n")

        data = response.json()
        console.print("[bold]Response:[/bold]")
        console.print_json(data=data)

        # Validate response
        if data.get("subject") == f"acct:{username}@{domain}":
            console.print("\n[green]✓ WebFinger endpoint is working correctly![/green]")
        else:
            console.print("\n[red]✗ Unexpected subject in response[/red]")

    except httpx.HTTPStatusError as e:
        console.print(f"[red]✗ HTTP Error {e.response.status_code}[/red]")
        console.print(e.response.text)
    except httpx.RequestError as e:
        console.print(f"[red]✗ Request Error: {e}[/red]")
        console.print("[dim]Make sure the WebFinger worker is deployed[/dim]")
    except Exception as e:
        console.print(f"[red]✗ Error: {e}[/red]")


@test.command()
@click.option('--local', is_flag=True, help='Test local development server')
def actor(local):
    """Test ActivityPub Actor endpoint."""
    config = Config()
    config.load()

    if local:
        activitypub_domain = "localhost:8788"
        protocol = "http"
    else:
        activitypub_domain = config.get("server.activitypub_domain", "social.dais.social")
        protocol = "https"

    username = config.get("server.username", "marc")

    console.print(f"[bold blue]Testing Actor endpoint...[/bold blue]\n")

    url = f"{protocol}://{activitypub_domain}/users/{username}"
    headers = {"Accept": "application/activity+json"}

    console.print(f"[dim]GET {url}[/dim]")
    console.print(f"[dim]Accept: application/activity+json[/dim]\n")

    try:
        response = httpx.get(url, headers=headers, timeout=10.0)
        response.raise_for_status()

        console.print(f"[green]✓[/green] Status: {response.status_code}")
        console.print(f"[green]✓[/green] Content-Type: {response.headers.get('content-type')}\n")

        data = response.json()
        console.print("[bold]Response:[/bold]")
        console.print_json(data=data)

        console.print("\n[green]✓ Actor endpoint is working![/green]")

    except httpx.HTTPStatusError as e:
        console.print(f"[red]✗ HTTP Error {e.response.status_code}[/red]")
        if e.response.status_code == 404:
            console.print("[dim]Actor endpoint not yet implemented[/dim]")
    except httpx.RequestError as e:
        console.print(f"[red]✗ Request Error: {e}[/red]")
    except Exception as e:
        console.print(f"[red]✗ Error: {e}[/red]")


@test.command()
@click.argument('actor')
@click.option('--local', is_flag=True, help='Test against local development servers')
def federation(actor, local):
    """Test federation with another instance.

    ACTOR: The ActivityPub actor to test (e.g., @user@mastodon.social)
    """
    console.print(f"[bold blue]Testing federation with {actor}...[/bold blue]\n")

    # Parse actor
    if actor.startswith("@"):
        actor = actor[1:]

    if "@" not in actor:
        console.print("[red]Invalid actor format. Expected @user@domain[/red]")
        return

    username, domain = actor.split("@", 1)

    # Test WebFinger lookup
    console.print("[bold]1. WebFinger lookup[/bold]")

    # Use local development server if --local flag is set
    if local:
        webfinger_url = f"http://localhost:8787/.well-known/webfinger"
        params = {"resource": f"acct:{username}@{domain}"}
    else:
        webfinger_url = f"https://{domain}/.well-known/webfinger"
        params = {"resource": f"acct:{username}@{domain}"}

    url = webfinger_url

    try:
        response = httpx.get(url, params=params, timeout=10.0)
        response.raise_for_status()
        data = response.json()

        # Find ActivityPub actor URL
        actor_url = None
        for link in data.get("links", []):
            if link.get("rel") == "self" and link.get("type") == "application/activity+json":
                actor_url = link.get("href")
                break

        if actor_url:
            console.print(f"[green]✓[/green] Found actor: {actor_url}\n")

            # Fetch actor
            console.print("[bold]2. Fetching Actor[/bold]")
            actor_response = httpx.get(
                actor_url,
                headers={"Accept": "application/activity+json"},
                timeout=10.0
            )
            actor_response.raise_for_status()
            actor_data = actor_response.json()

            console.print(f"[green]✓[/green] Actor type: {actor_data.get('type')}")
            console.print(f"[green]✓[/green] Inbox: {actor_data.get('inbox')}")
            console.print(f"[green]✓[/green] Outbox: {actor_data.get('outbox')}")

            console.print("\n[green]✓ Federation test successful![/green]")
            console.print(f"[dim]You can now try following @{username}@{domain}[/dim]")
        else:
            console.print("[red]✗ No ActivityPub actor link found in WebFinger[/red]")

    except httpx.HTTPStatusError as e:
        console.print(f"[red]✗ HTTP Error {e.response.status_code}[/red]")
    except httpx.RequestError as e:
        console.print(f"[red]✗ Request Error: {e}[/red]")
    except Exception as e:
        console.print(f"[red]✗ Error: {e}[/red]")
