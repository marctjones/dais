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
    remote: bool = False,
    visibility: str = "public"
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
        visibility: Post visibility ('public', 'unlisted', 'followers', 'direct')

    Returns:
        Dict with delivery results
    """
    results = {
        'activitypub': {'successful': 0, 'failed': 0},
        'atproto': {'success': False, 'uri': None}
    }

    # PRIVACY SAFETY CHECK: Prevent leaking private posts to Bluesky
    # Bluesky doesn't support followers-only or DM visibility - all posts are public
    original_protocol = protocol
    if protocol in ('atproto', 'both') and visibility in ('followers', 'direct'):
        console.print(f"[yellow]⚠[/yellow]  Privacy Notice: Bluesky doesn't support '{visibility}' visibility")
        console.print(f"[yellow]   All Bluesky posts are public. Posting to ActivityPub only to protect your privacy.[/yellow]")
        protocol = 'activitypub'  # Override to ActivityPub only
        results['atproto'] = {'success': False, 'uri': None, 'skipped': 'privacy_protection'}

        # Update database to reflect actual protocol used
        if original_protocol == 'both':
            project_root = Path(__file__).parent.parent.parent
            worker_dir = project_root / "workers" / "actor"
            post_id_escaped = post_id.replace("'", "''")
            update_query = f"UPDATE posts SET protocol = 'activitypub' WHERE id = '{post_id_escaped}'"
            cmd = ["wrangler", "d1", "execute", "DB", "--command", update_query]
            if remote:
                cmd.append("--remote")
            else:
                cmd.append("--local")
            try:
                subprocess.run(cmd, capture_output=True, text=True, check=True, cwd=str(worker_dir))
            except subprocess.CalledProcessError:
                pass  # Non-critical: database update failed but delivery will still work

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
    """Post to self-hosted PDS via AT Protocol.

    Args:
        text: Post text content
        post_id: ID of the post being delivered
        remote: Whether this is for production

    Returns:
        AT Protocol URI of the created post, or None on failure
    """
    # Use self-hosted PDS
    pds_url = "https://pds.dais.social" if remote else "http://localhost:8787"

    # Load PDS credentials
    password_path = get_dais_dir() / "pds-password.txt"
    if not password_path.exists():
        console.print("[yellow]⚠[/yellow] PDS password not found. Check .dais/pds-password.txt")
        return None

    with open(password_path) as f:
        password = f.read().strip()

    # Create session with self-hosted PDS
    import httpx

    try:
        # Authenticate
        auth_response = httpx.post(
            f"{pds_url}/xrpc/com.atproto.server.createSession",
            json={
                "identifier": "social.dais.social",
                "password": password
            },
            timeout=30.0
        )

        if auth_response.status_code != 200:
            console.print(f"[yellow]⚠[/yellow] PDS authentication failed: {auth_response.status_code}")
            return None

        session = auth_response.json()
        access_token = session.get("access_jwt")
        did = session.get("did", "did:web:social.dais.social")

        # Create post record
        from datetime import datetime
        created_at = datetime.utcnow().isoformat() + "Z"

        post_response = httpx.post(
            f"{pds_url}/xrpc/com.atproto.repo.createRecord",
            json={
                "repo": did,
                "collection": "app.bsky.feed.post",
                "record": {
                    "$type": "app.bsky.feed.post",
                    "text": text,
                    "createdAt": created_at
                }
            },
            headers={"Authorization": f"Bearer {access_token}"},
            timeout=30.0
        )

        if post_response.status_code == 200:
            result = post_response.json()
            uri = result.get("uri")
            cid = result.get("cid")

            # Update database with AT Protocol URI
            if uri:
                project_root = Path(__file__).parent.parent.parent
                worker_dir = project_root / "workers" / "actor"

                atproto_uri_escaped = uri.replace("'", "''")
                atproto_cid_escaped = cid.replace("'", "''") if cid else ''

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

                return uri
        else:
            console.print(f"[yellow]⚠[/yellow] Failed to create post: {post_response.status_code}")

    except Exception as e:
        console.print(f"[yellow]⚠[/yellow] PDS error: {e}")

    return None
