"""Send ActivityPub activities to remote inboxes."""

import click
import httpx
import json
from pathlib import Path
from datetime import datetime
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import padding
from cryptography.hazmat.backends import default_backend
import base64
from rich.console import Console

from dais_cli.config import Config

console = Console()


def sign_request(url: str, body: str, private_key_path: Path, key_id: str) -> dict:
    """Sign an HTTP POST request with HTTP Signatures."""
    
    # Load private key
    with open(private_key_path, 'rb') as f:
        private_key = serialization.load_pem_private_key(
            f.read(),
            password=None,
            backend=default_backend()
        )
    
    # Parse URL to get host and path
    from urllib.parse import urlparse
    parsed = urlparse(url)
    host = parsed.netloc
    path = parsed.path
    
    # Get current date in HTTP format
    date = datetime.utcnow().strftime('%a, %d %b %Y %H:%M:%S GMT')
    
    # Calculate digest of body
    import hashlib
    body_hash = hashlib.sha256(body.encode('utf-8')).digest()
    digest = 'SHA-256=' + base64.b64encode(body_hash).decode('utf-8')
    
    # Build signing string
    signing_string = f"(request-target): post {path}\nhost: {host}\ndate: {date}\ndigest: {digest}"
    
    # Sign the string
    signature_bytes = private_key.sign(
        signing_string.encode('utf-8'),
        padding.PKCS1v15(),
        hashes.SHA256()
    )
    signature_b64 = base64.b64encode(signature_bytes).decode('utf-8')
    
    # Build signature header
    signature_header = (
        f'keyId="{key_id}",'
        f'algorithm="rsa-sha256",'
        f'headers="(request-target) host date digest",'
        f'signature="{signature_b64}"'
    )
    
    return {
        'Date': date,
        'Digest': digest,
        'Signature': signature_header,
        'Content-Type': 'application/activity+json',
        'Accept': 'application/activity+json'
    }


@click.command()
@click.argument('activity_type', type=click.Choice(['Accept', 'Reject']))
@click.argument('follow_activity_id')
@click.option('--remote', is_flag=True, help='Query remote database')
def send_response(activity_type, follow_activity_id, remote):
    """Send Accept or Reject activity in response to a Follow.
    
    ACTIVITY_TYPE: Accept or Reject
    FOLLOW_ACTIVITY_ID: The ID of the Follow activity to respond to
    """
    import subprocess
    
    config = Config.load()
    
    console.print(f"[bold blue]Sending {activity_type} for Follow activity[/bold blue]\n")
    
    # Find project root
    project_root = Path(__file__).parent.parent.parent.parent
    worker_dir = project_root / "workers" / "actor"
    
    # Get the Follow activity from database
    query = f"SELECT id, follower_actor_id, follower_inbox FROM followers WHERE id = '{follow_activity_id}'"
    cmd = ["wrangler", "d1", "execute", "dais-social", "--command", query, "--json"]
    if remote:
        cmd.append("--remote")
    
    result = subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
    
    # Parse result
    data = json.loads(result.stdout)
    results = data[0].get("results", [])
    
    if not results:
        console.print(f"[red]✗ Follow activity not found: {follow_activity_id}[/red]")
        return
    
    follower = results[0]
    follower_inbox = follower['follower_inbox']
    follower_actor = follower['follower_actor_id']
    original_follow_id = follower['id']
    
    console.print(f"[dim]Follower: {follower_actor}[/dim]")
    console.print(f"[dim]Inbox: {follower_inbox}[/dim]\n")
    
    # Build Accept/Reject activity
    activity_id = f"https://social.dais.social/activities/{datetime.utcnow().strftime('%Y%m%d%H%M%S')}"
    our_actor = "https://social.dais.social/users/marc"
    
    activity = {
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": activity_type,
        "id": activity_id,
        "actor": our_actor,
        "object": {
            "type": "Follow",
            "id": original_follow_id,
            "actor": follower_actor,
            "object": our_actor
        }
    }
    
    body = json.dumps(activity)
    
    # Sign the request
    private_key_path = Path.home() / ".dais" / "keys" / "private.pem"
    key_id = f"{our_actor}#main-key"
    
    headers = sign_request(follower_inbox, body, private_key_path, key_id)
    
    console.print(f"[dim]Sending signed {activity_type} activity...[/dim]")
    
    # Send to follower's inbox
    try:
        response = httpx.post(
            follower_inbox,
            headers=headers,
            content=body,
            timeout=30.0
        )
        
        if response.status_code in [200, 202]:
            console.print(f"[green]✓[/green] {activity_type} activity sent successfully!")
            console.print(f"[dim]Response: {response.status_code}[/dim]")
        else:
            console.print(f"[yellow]⚠[/yellow] Got response {response.status_code}")
            console.print(f"[dim]{response.text}[/dim]")
            
    except Exception as e:
        console.print(f"[red]✗ Failed to send activity[/red]")
        console.print(f"[red]{str(e)}[/red]")
