"""Activity delivery to remote ActivityPub instances."""

import httpx
import json
import base64
import hashlib
from pathlib import Path
from datetime import datetime
from typing import Optional, Dict, Any, Tuple, List
from urllib.parse import urlparse
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import padding
from cryptography.hazmat.backends import default_backend
from rich.console import Console

console = Console()


def sign_and_send_activity(
    activity: Dict[str, Any],
    inbox_url: str,
    actor_url: str,
    private_key_path: Optional[Path] = None
) -> Tuple[bool, Optional[int]]:
    """Sign and send an ActivityPub activity to a remote inbox.

    Args:
        activity: The ActivityPub activity to send (dict)
        inbox_url: The inbox URL to send to
        actor_url: Our actor URL (for keyId)
        private_key_path: Path to private key (defaults to ~/.dais/keys/private.pem)

    Returns:
        Tuple of (success: bool, status_code: Optional[int])
    """
    if private_key_path is None:
        private_key_path = Path.home() / ".dais" / "keys" / "private.pem"

    # Serialize activity to JSON
    body = json.dumps(activity)

    # Load private key
    with open(private_key_path, 'rb') as f:
        private_key = serialization.load_pem_private_key(
            f.read(),
            password=None,
            backend=default_backend()
        )

    # Parse inbox URL
    parsed = urlparse(inbox_url)
    host = parsed.netloc
    path = parsed.path

    # Build HTTP signature components
    date = datetime.utcnow().strftime('%a, %d %b %Y %H:%M:%S GMT')

    # Compute digest
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

    # Build Signature header
    key_id = f"{actor_url}#main-key"
    signature_header = (
        f'keyId="{key_id}",'
        f'algorithm="rsa-sha256",'
        f'headers="(request-target) host date digest",'
        f'signature="{signature_b64}"'
    )

    # Build HTTP headers
    headers = {
        'Date': date,
        'Digest': digest,
        'Signature': signature_header,
        'Content-Type': 'application/activity+json',
        'Accept': 'application/activity+json'
    }

    # Send the activity
    try:
        response = httpx.post(inbox_url, headers=headers, content=body, timeout=30.0)
        success = response.status_code in [200, 202]
        return success, response.status_code
    except Exception as e:
        console.print(f"[red]✗ Error sending activity to {inbox_url}: {e}[/red]")
        return False, None


def deliver_to_followers(
    activity: Dict[str, Any],
    followers: List[Dict[str, str]],
    actor_url: str,
    private_key_path: Optional[Path] = None,
    verbose: bool = False
) -> Tuple[int, int]:
    """Deliver an activity to multiple followers.

    Args:
        activity: The ActivityPub activity to send
        followers: List of follower dicts with 'follower_inbox' and 'follower_actor_id'
        actor_url: Our actor URL (for keyId)
        private_key_path: Path to private key (defaults to ~/.dais/keys/private.pem)
        verbose: Print delivery status for each follower

    Returns:
        Tuple of (successful_deliveries: int, failed_deliveries: int)
    """
    successful = 0
    failed = 0

    for follower in followers:
        inbox = follower.get('follower_inbox')
        actor_id = follower.get('follower_actor_id', 'unknown')

        if not inbox:
            console.print(f"[yellow]⚠ Skipping follower (no inbox): {actor_id}[/yellow]")
            failed += 1
            continue

        success, status = sign_and_send_activity(
            activity=activity,
            inbox_url=inbox,
            actor_url=actor_url,
            private_key_path=private_key_path
        )

        if success:
            successful += 1
            if verbose:
                console.print(f"[green]✓ Delivered to {actor_id} ({status})[/green]")
        else:
            failed += 1
            if verbose:
                console.print(f"[red]✗ Failed to deliver to {actor_id} ({status})[/red]")

    return successful, failed


def build_accept_activity(actor_url: str, follow_activity: Dict[str, Any]) -> Dict[str, Any]:
    """Build an Accept activity for a Follow request.

    Args:
        actor_url: Our actor URL
        follow_activity: The original Follow activity to accept

    Returns:
        Accept activity dict
    """
    activity_id = f"https://social.dais.social/activities/{datetime.utcnow().strftime('%Y%m%d%H%M%S')}"

    return {
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Accept",
        "id": activity_id,
        "actor": actor_url,
        "object": follow_activity
    }


def build_reject_activity(actor_url: str, follow_activity: Dict[str, Any]) -> Dict[str, Any]:
    """Build a Reject activity for a Follow request.

    Args:
        actor_url: Our actor URL
        follow_activity: The original Follow activity to reject

    Returns:
        Reject activity dict
    """
    activity_id = f"https://social.dais.social/activities/{datetime.utcnow().strftime('%Y%m%d%H%M%S')}"

    return {
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Reject",
        "id": activity_id,
        "actor": actor_url,
        "object": follow_activity
    }


def build_create_activity(actor_url: str, note: Dict[str, Any]) -> Dict[str, Any]:
    """Build a Create activity for a Note (post).

    Args:
        actor_url: Our actor URL
        note: The Note object to wrap in Create activity

    Returns:
        Create activity dict
    """
    activity_id = f"{note['id']}/activity"

    return {
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Create",
        "id": activity_id,
        "actor": actor_url,
        "published": note.get("published", datetime.utcnow().isoformat() + "Z"),
        "to": note.get("to", []),
        "cc": note.get("cc", []),
        "object": note
    }


def build_delete_activity(actor_url: str, object_url: str) -> Dict[str, Any]:
    """Build a Delete activity for an object (post).

    Args:
        actor_url: Our actor URL
        object_url: URL of the object to delete

    Returns:
        Delete activity dict
    """
    activity_id = f"https://social.dais.social/activities/{datetime.utcnow().strftime('%Y%m%d%H%M%S')}"

    return {
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Delete",
        "id": activity_id,
        "actor": actor_url,
        "object": object_url,
        "to": ["https://www.w3.org/ns/activitystreams#Public"]
    }


def deliver_activity_to_inbox(
    activity: Dict[str, Any],
    inbox_url: str,
    actor_url: str,
    private_key_path: Optional[Path] = None
) -> bool:
    """Convenience function to deliver an activity to a remote inbox.

    Args:
        activity: The ActivityPub activity to send
        inbox_url: The inbox URL to send to
        actor_url: Our actor URL (for keyId)
        private_key_path: Path to private key (defaults to ~/.dais/keys/private.pem)

    Returns:
        True if delivery was successful, False otherwise
    """
    success, status_code = sign_and_send_activity(
        activity=activity,
        inbox_url=inbox_url,
        actor_url=actor_url,
        private_key_path=private_key_path
    )
    return success
