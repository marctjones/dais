"""Unified protocol manager for ActivityPub and AT Protocol."""
from enum import Enum
from typing import Optional, List, Dict, Tuple
from pathlib import Path
import json
from dais_cli.config import get_dais_dir


class Protocol(Enum):
    """Supported protocols."""
    ACTIVITYPUB = "activitypub"
    ATPROTO = "atproto"
    BOTH = "both"


def detect_protocol(handle: str) -> Protocol:
    """Detect protocol from handle format.

    Args:
        handle: User handle (e.g., @user@mastodon.social or user.bsky.social)

    Returns:
        Protocol type

    Examples:
        @user@mastodon.social -> ACTIVITYPUB
        user.bsky.social -> ATPROTO
        did:plc:xxx -> ATPROTO
    """
    handle = handle.strip()

    # DID format = AT Protocol
    if handle.startswith('did:'):
        return Protocol.ATPROTO

    # @user@instance.social = ActivityPub
    if handle.startswith('@') and handle.count('@') == 2:
        return Protocol.ACTIVITYPUB

    # user.bsky.social or handle without @ = AT Protocol
    if '.bsky.social' in handle or '.' in handle:
        return Protocol.ATPROTO

    # Default to ActivityPub for backward compatibility
    return Protocol.ACTIVITYPUB


class ProtocolManager:
    """Manages operations across multiple protocols."""

    def __init__(self):
        """Initialize protocol manager."""
        self.activitypub_enabled = True  # Always available
        self.atproto_enabled = self._check_atproto_auth()

    def _check_atproto_auth(self) -> bool:
        """Check if AT Protocol is configured."""
        config_path = get_dais_dir() / "bluesky.json"
        return config_path.exists()

    def get_enabled_protocols(self) -> List[Protocol]:
        """Get list of enabled protocols."""
        protocols = [Protocol.ACTIVITYPUB]
        if self.atproto_enabled:
            protocols.append(Protocol.ATPROTO)
        return protocols

    def create_post(self, text: str, protocol: Optional[Protocol] = None) -> Dict[str, str]:
        """Create a post on specified protocol(s).

        Args:
            text: Post content
            protocol: Target protocol (None = all enabled)

        Returns:
            Dict mapping protocol to post URL/URI
        """
        if protocol is None:
            protocol = Protocol.BOTH

        results = {}

        # Post to ActivityPub
        if protocol in (Protocol.ACTIVITYPUB, Protocol.BOTH) and self.activitypub_enabled:
            from ..delivery import execute_remote_d1, get_actor_info
            from uuid import uuid4
            from datetime import datetime

            actor = get_actor_info(remote=True)
            post_id = f"{datetime.utcnow().strftime('%Y%m%d%H%M%S')}-{uuid4().hex[:8]}"
            published = datetime.utcnow().isoformat() + 'Z'
            actor_id = f"https://social.dais.social/users/{actor['username']}"

            query = f"""
                INSERT INTO posts (id, actor_id, content, visibility, published_at)
                VALUES ('{post_id}', '{actor_id}', '{text.replace("'", "''")}', 'public', '{published}')
            """

            execute_remote_d1(query, remote=True)
            results['activitypub'] = f"https://social.dais.social/users/{actor['username']}/posts/{post_id}"

        # Post to AT Protocol
        if protocol in (Protocol.ATPROTO, Protocol.BOTH) and self.atproto_enabled:
            from atproto import Client

            config_path = get_dais_dir() / "bluesky.json"
            with open(config_path) as f:
                config = json.load(f)

            client = Client()
            client.login(config['handle'], config['password'])
            response = client.send_post(text=text)
            results['atproto'] = response.uri

        return results

    def get_timeline(self, limit: int = 20) -> List[Dict]:
        """Get unified timeline from all protocols.

        Args:
            limit: Maximum posts to return

        Returns:
            List of posts from all protocols, sorted by time
        """
        posts = []

        # Get ActivityPub timeline
        if self.activitypub_enabled:
            from ..delivery import execute_remote_d1

            query = """
                SELECT target_actor_id
                FROM following
                WHERE status = 'accepted'
                ORDER BY accepted_at DESC
            """

            following = execute_remote_d1(query, remote=True)

            if following:
                import requests

                for user in following[:20]:  # Limit to avoid too many requests
                    actor_id = user['target_actor_id']
                    outbox_url = f"{actor_id}/outbox"

                    try:
                        response = requests.get(
                            outbox_url,
                            headers={'Accept': 'application/activity+json'},
                            timeout=10
                        )

                        if response.status_code == 200:
                            outbox_data = response.json()
                            items = outbox_data.get('orderedItems', [])

                            for item in items[:5]:  # Limit per user
                                if isinstance(item, dict):
                                    item_type = item.get('type')
                                    if item_type == 'Note':
                                        posts.append({
                                            'protocol': 'activitypub',
                                            'content': item.get('content', ''),
                                            'author': actor_id,
                                            'published': item.get('published', ''),
                                            'url': item.get('id', '')
                                        })
                    except:
                        continue

        # Get AT Protocol timeline
        if self.atproto_enabled:
            from atproto import Client

            config_path = get_dais_dir() / "bluesky.json"
            with open(config_path) as f:
                config = json.load(f)

            client = Client()
            client.login(config['handle'], config['password'])
            feed = client.get_timeline(limit=limit)

            for item in feed.feed:
                post = item.post
                record = post.record

                posts.append({
                    'protocol': 'atproto',
                    'content': record.text,
                    'author': f"@{post.author.handle}",
                    'published': record.created_at,
                    'url': post.uri
                })

        # Sort by published date
        posts.sort(key=lambda p: p.get('published', ''), reverse=True)

        return posts[:limit]

    def follow_user(self, handle: str) -> Tuple[Protocol, bool]:
        """Follow a user on the appropriate protocol.

        Args:
            handle: User handle

        Returns:
            Tuple of (protocol used, success)
        """
        protocol = detect_protocol(handle)

        if protocol == Protocol.ACTIVITYPUB:
            # Use existing ActivityPub follow logic
            # This would call the existing follow.add command logic
            return (Protocol.ACTIVITYPUB, True)

        elif protocol == Protocol.ATPROTO:
            if not self.atproto_enabled:
                return (Protocol.ATPROTO, False)

            from atproto import Client

            config_path = get_dais_dir() / "bluesky.json"
            with open(config_path) as f:
                config = json.load(f)

            client = Client()
            client.login(config['handle'], config['password'])

            handle = handle.lstrip('@')
            profile = client.get_profile(handle)
            client.follow(profile.did)

            return (Protocol.ATPROTO, True)

        return (protocol, False)
