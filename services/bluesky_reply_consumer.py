"""Bluesky Reply Consumer - Subscribe to Bluesky firehose and capture incoming replies.

This service connects to the Bluesky relay firehose and listens for replies to our posts.
When a Bluesky user replies to one of our dual-protocol posts, this captures it and
stores it in the local database alongside ActivityPub replies.

Usage:
    python bluesky_reply_consumer.py [--local|--remote]

Requirements:
    pip install atproto websockets cbor2
"""

import asyncio
import json
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from typing import Optional, Dict, Any

try:
    from atproto import AsyncFirehoseSubscribeReposClient, parse_subscribe_repos_message, models, firehose_models
    ATPROTO_AVAILABLE = True
except ImportError:
    ATPROTO_AVAILABLE = False
    print("⚠️  atproto library not available. Install with: pip install atproto")


class BlueskyReplyConsumer:
    """Consumer for Bluesky firehose that captures replies to our posts."""

    def __init__(self, remote: bool = False):
        """Initialize the consumer.

        Args:
            remote: If True, use production database. If False, use local database.
        """
        self.remote = remote
        self.worker_dir = Path(__file__).parent.parent / "workers" / "actor"

        # Cache of our post URIs (atproto_uri -> activitypub post_id)
        self.our_posts: Dict[str, str] = {}

        # Stats
        self.commits_processed = 0
        self.replies_stored = 0

    def load_our_posts(self):
        """Load our AT Protocol post URIs from database."""
        print("Loading our posts from database...")

        query = "SELECT id, atproto_uri FROM posts WHERE atproto_uri IS NOT NULL AND atproto_uri != '';"
        cmd = ["wrangler", "d1", "execute", "DB", "--command", query]
        if self.remote:
            cmd.append("--remote")
        else:
            cmd.append("--local")

        try:
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                check=True,
                cwd=str(self.worker_dir)
            )

            # Parse JSON from wrangler output
            start = result.stdout.find('[')
            end = result.stdout.rfind(']') + 1
            if start >= 0 and end > 0:
                data = json.loads(result.stdout[start:end])
                if data and len(data) > 0 and "results" in data[0]:
                    for post in data[0]["results"]:
                        atproto_uri = post.get("atproto_uri")
                        post_id = post.get("id")
                        if atproto_uri and post_id:
                            self.our_posts[atproto_uri] = post_id

                    print(f"✓ Loaded {len(self.our_posts)} posts with AT Protocol URIs")
                    for uri in self.our_posts.keys():
                        print(f"  - {uri}")
                else:
                    print("No posts with AT Protocol URIs found")
        except subprocess.CalledProcessError as e:
            print(f"✗ Failed to load posts: {e}")
            if e.stderr:
                print(f"stderr: {e.stderr}")

    def extract_reply_parent(self, record: models.AppBskyFeedPost.Record) -> Optional[str]:
        """Extract the parent post URI from a reply record.

        Args:
            record: The post record from the firehose

        Returns:
            Parent post URI if this is a reply, None otherwise
        """
        if not hasattr(record, 'reply') or not record.reply:
            return None

        if hasattr(record.reply, 'parent') and hasattr(record.reply.parent, 'uri'):
            return record.reply.parent.uri

        return None

    def store_reply(
        self,
        reply_uri: str,
        parent_post_id: str,
        actor_did: str,
        record: models.AppBskyFeedPost.Record
    ):
        """Store a Bluesky reply in the database.

        Args:
            reply_uri: AT Protocol URI of the reply
            parent_post_id: Our ActivityPub post ID this is replying to
            actor_did: DID of the Bluesky user who replied
            record: The post record with reply content
        """
        # Extract content
        text = record.text if hasattr(record, 'text') else ""
        created_at = record.created_at if hasattr(record, 'created_at') else datetime.utcnow().isoformat() + "Z"

        # Convert created_at to string if it's a datetime
        if isinstance(created_at, datetime):
            created_at = created_at.isoformat() + "Z"

        # Extract handle (simplified - would need DID resolution in production)
        actor_handle = actor_did.replace("did:plc:", "user") if "did:plc:" in actor_did else "unknown"
        actor_username = f"@{actor_handle}.bsky.social"

        # Escape SQL strings
        reply_uri_escaped = reply_uri.replace("'", "''")
        parent_post_id_escaped = parent_post_id.replace("'", "''")
        actor_did_escaped = actor_did.replace("'", "''")
        actor_username_escaped = actor_username.replace("'", "''")
        text_escaped = text.replace("'", "''")
        created_at_escaped = created_at.replace("'", "''")

        # Insert into replies table
        query = f"""
        INSERT INTO replies (id, post_id, actor_id, actor_username, content, published_at, moderation_status, moderation_score, hidden)
        VALUES ('{reply_uri_escaped}', '{parent_post_id_escaped}', '{actor_did_escaped}', '{actor_username_escaped}', '{text_escaped}', '{created_at_escaped}', 'approved', 0.0, 0)
        ON CONFLICT(id) DO NOTHING;
        """

        cmd = ["wrangler", "d1", "execute", "DB", "--command", query]
        if self.remote:
            cmd.append("--remote")
        else:
            cmd.append("--local")

        try:
            subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                check=True,
                cwd=str(self.worker_dir)
            )
            self.replies_stored += 1
            print(f"✓ [{self.replies_stored}] Stored Bluesky reply from {actor_username}")
            print(f"  Reply: {text[:60]}...")
            print(f"  To post: {parent_post_id}")
        except subprocess.CalledProcessError as e:
            print(f"✗ Failed to store reply: {e}")
            if e.stderr:
                print(f"stderr: {e.stderr}")

    def handle_commit(self, commit: models.ComAtprotoSyncSubscribeRepos.Commit):
        """Handle a commit event from the firehose.

        Args:
            commit: The commit event data
        """
        self.commits_processed += 1

        # Print progress every 100 commits
        if self.commits_processed % 100 == 0:
            print(f"[Stats] Processed {self.commits_processed} commits, stored {self.replies_stored} replies")

        # Get repo (actor DID)
        repo = commit.repo

        # Decode CAR blocks to get records
        if not hasattr(commit, 'blocks') or not commit.blocks:
            return  # No blocks to process

        try:
            # Use the atproto library's CAR reader
            from atproto import CAR
            from io import BytesIO

            # Read the CAR file from commit blocks
            car_file = BytesIO(commit.blocks)
            car_reader = CAR.from_bytes(commit.blocks)

            # Parse operations from commit
            for op in commit.ops:
                # Only care about creates
                if op.action != 'create':
                    continue

                # Only care about posts
                if not op.path.startswith('app.bsky.feed.post/'):
                    continue

                # Try to get the record from CAR blocks
                try:
                    # The CID is in the operation
                    if not hasattr(op, 'cid') or not op.cid:
                        continue

                    # Get the record from the CAR reader
                    # The car_reader.blocks is a dict of CID -> bytes
                    cid_str = str(op.cid) if hasattr(op.cid, '__str__') else op.cid

                    # Try different ways to access the block
                    record_data = None
                    if hasattr(car_reader, 'blocks'):
                        record_data = car_reader.blocks.get(cid_str)

                    if not record_data:
                        # Try using the CID object directly
                        record_data = car_reader.blocks.get(op.cid)

                    if not record_data:
                        continue

                    # Decode the CBOR data to get the record
                    import cbor2
                    record_dict = cbor2.loads(record_data)

                    # Create a post record from the dictionary
                    # The record should have fields like 'text', 'reply', 'createdAt'
                    record = models.AppBskyFeedPost.Record(**record_dict)

                    # Construct the reply URI
                    reply_uri = f"at://{repo}/{op.path}"

                    # Check if this is a reply to one of our posts
                    parent_uri = self.extract_reply_parent(record)
                    if not parent_uri:
                        continue  # Not a reply

                    # Check if the parent is one of our posts
                    parent_post_id = self.our_posts.get(parent_uri)
                    if not parent_post_id:
                        continue  # Not replying to our post

                    # Store the reply!
                    self.store_reply(reply_uri, parent_post_id, repo, record)

                except Exception as e:
                    # Skip records we can't parse
                    # Don't print errors for every failed parse (too noisy)
                    if self.commits_processed % 1000 == 0:
                        error_type = type(e).__name__
                        print(f"[Debug] Record parse error ({error_type}): {str(e)[:100]}")
                        print(f"        Repo: {repo}, Path: {op.path if hasattr(op, 'path') else 'unknown'}")
                    pass

        except Exception as e:
            # Failed to decode CAR blocks - skip this commit
            if self.commits_processed % 1000 == 0:
                error_type = type(e).__name__
                print(f"[Debug] CAR decode error ({error_type}): {str(e)[:100]}")
                print(f"        Commit from repo: {commit.repo if hasattr(commit, 'repo') else 'unknown'}")
            pass

    async def run_firehose(self):
        """Connect to Bluesky firehose and consume events."""
        print("=" * 60)
        print("Bluesky Reply Consumer - Starting")
        print("=" * 60)
        print(f"Monitoring {len(self.our_posts)} post(s) for replies")
        print("Connecting to Bluesky relay firehose (wss://bsky.network)...")
        print("")

        # Use the official atproto client
        client = AsyncFirehoseSubscribeReposClient()

        async def on_message_handler(message: firehose_models.MessageFrame):
            """Handle incoming firehose messages."""
            try:
                commit = parse_subscribe_repos_message(message)

                if not isinstance(commit, models.ComAtprotoSyncSubscribeRepos.Commit):
                    return

                self.handle_commit(commit)
            except Exception as e:
                # Log message handler errors (very rare)
                print(f"[Error] Message handler exception: {type(e).__name__}: {e}")

        # Subscribe with auto-reconnect
        try:
            print("✓ Connected to firehose")
            print("Listening for replies... (Press Ctrl+C to stop)")
            print("")
            await client.start(on_message_handler)
        except KeyboardInterrupt:
            print("\nShutdown signal received")
            raise
        except Exception as e:
            print(f"[Error] Firehose connection failed: {type(e).__name__}: {e}")
            raise

    def run(self):
        """Run the consumer."""
        self.load_our_posts()

        if not self.our_posts:
            print("⚠️  No posts with AT Protocol URIs found.")
            print("   Create a dual-protocol post first:")
            print("   dais post create 'Hello Bluesky!' --protocol both")
            print("")
            print("Exiting...")
            return

        print("")
        print("Starting firehose consumer...")
        print("Press Ctrl+C to stop")
        print("")

        if ATPROTO_AVAILABLE:
            asyncio.run(self.run_firehose())
        else:
            print("✗ Cannot start: atproto library not available")
            print("  Install with: pip install atproto")


def main():
    """Main entry point."""
    # Parse command line args
    remote = "--remote" in sys.argv

    mode = "PRODUCTION" if remote else "LOCAL"
    print("=" * 60)
    print(f"Bluesky Reply Consumer - {mode} Mode")
    print("=" * 60)
    print("")

    if not ATPROTO_AVAILABLE:
        print("✗ Missing required library")
        print("  Install with: pip install atproto")
        sys.exit(1)

    consumer = BlueskyReplyConsumer(remote=remote)
    consumer.run()


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n\nShutting down...")
