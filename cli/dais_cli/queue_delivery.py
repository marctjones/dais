"""Simplified delivery for both ActivityPub and AT Protocol.

For now, uses existing synchronous delivery with dual-protocol support.
Queue-based async delivery will be implemented later with proper worker infrastructure.
"""

import json
import subprocess
import uuid
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Optional

from rich.console import Console
from dais_cli.delivery import deliver_to_followers
from dais_cli.config import get_dais_dir

console = Console()


def deliver_dual_protocol_post(
    text: str,
    post_id: str,
    actor_url: str,
    activity: Dict,
    followers: List[Dict],
    protocol: str = "both",
    remote: bool = False
) -> Dict[str, any]:
    """Deliver post to selected protocol(s).

    Args:
        text: Post text content
        post_id: ID of the post
        actor_url: ActivityPub actor URL
        activity: ActivityPub activity object
        followers: List of ActivityPub followers
        protocol: Which protocol(s) to use ('activitypub', 'atproto', 'both')
        remote: Whether to use remote database

    Returns:
        Dict with delivery results
    """
    results = {
        'activitypub': {'successful': 0, 'failed': 0},
        'atproto': {'success': False, 'uri': None}
    }

    # Deliver to ActivityPub
    if protocol in ('activitypub', 'both') and followers:
        console.print(f"[dim]Delivering to {len(followers)} ActivityPub follower(s)...[/dim]")
        successful, failed = deliver_to_followers(
            activity=activity,
            followers=followers,
            actor_url=actor_url,
            verbose=True
        )
        results['activitypub'] = {'successful': successful, 'failed': failed}

    # Deliver to AT Protocol (Bluesky)
    if protocol in ('atproto', 'both'):
        console.print("[dim]Posting to Bluesky...[/dim]")
        try:
            uri = deliver_to_bluesky(text, post_id, remote)
            results['atproto'] = {'success': True, 'uri': uri}
            console.print(f"[green]✓[/green] Posted to Bluesky: {uri}")
        except Exception as e:
            console.print(f"[yellow]⚠[/yellow] Bluesky delivery failed: {e}")
            results['atproto'] = {'success': False, 'uri': None}

    return results


def deliver_to_bluesky(text: str, post_id: str, remote: bool = False) -> Optional[str]:
    """Post to Bluesky via AT Protocol.

    Args:
        text: Post text content
        post_id: ID of the post being delivered
        remote: Whether this is for production

    Returns:
        AT Protocol URI of the created post, or None on failure
    """
    try:
        from atproto import Client
    except ImportError:
        console.print("[yellow]⚠[/yellow] atproto library not installed. Install with: pip install atproto")
        return None

    # Load Bluesky credentials
    config_path = get_dais_dir() / "bluesky.json"
    if not config_path.exists():
        console.print("[yellow]⚠[/yellow] Bluesky not configured. Run: dais setup bluesky")
        return None

    with open(config_path) as f:
        config = json.load(f)

    # Create client and login
    client = Client()
    client.login(config['handle'], config['password'])

    # Send post
    response = client.send_post(text=text)

    # Update database with AT Protocol URI
    if response and response.uri:
        project_root = Path(__file__).parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        atproto_uri_escaped = response.uri.replace("'", "''")
        atproto_cid_escaped = response.cid.replace("'", "''") if hasattr(response, 'cid') else ''

        update_query = f"""
        UPDATE posts
        SET atproto_uri = '{atproto_uri_escaped}', atproto_cid = '{atproto_cid_escaped}'
        WHERE id = '{post_id.replace("'", "''")}'
        """

        cmd = ["wrangler", "d1", "execute", "DB", "--command", update_query]
        if remote:
            cmd.append("--remote")

        try:
            subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
        except subprocess.CalledProcessError:
            console.print("[dim]Warning: Could not save AT Protocol URI to database[/dim]")

        return response.uri

    return None
