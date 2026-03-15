"""Follower list widget for dashboard."""

import subprocess
import json
from pathlib import Path
from textual.app import ComposeResult
from textual.widgets import Static, ListView, ListItem, Label
from textual.containers import Vertical


class FollowerListWidget(Static):
    """Display followers and pending requests."""

    DEFAULT_CSS = """
    FollowerListWidget {
        height: auto;
        max-height: 15;
        border: solid $primary;
        padding: 1;
    }
    """

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.followers = []

    def compose(self) -> ComposeResult:
        """Compose the follower list."""
        with Vertical():
            yield Static("👥 **Followers**", classes="list-header")
            yield ListView(id="follower-list-view")

    def refresh_data(self) -> None:
        """Fetch followers from database."""
        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        try:
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 "SELECT follower_actor_id, status FROM followers LIMIT 10;"],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )

            if result.returncode == 0:
                data = json.loads(result.stdout)
                if data and len(data) > 0 and "results" in data[0]:
                    self.followers = data[0]["results"]

        except Exception:
            self.followers = []

        self._update_display()

    def _update_display(self) -> None:
        """Update the follower list display."""
        list_view = self.query_one("#follower-list-view", ListView)
        list_view.clear()

        if not self.followers:
            list_view.append(ListItem(Label("No followers yet")))
            return

        for follower in self.followers:
            actor_id = follower.get("follower_actor_id", "")
            status = follower.get("status", "unknown")

            # Extract username from actor ID
            # e.g., https://mastodon.social/users/alice -> alice@mastodon.social
            if "/" in actor_id:
                parts = actor_id.split("/")
                domain = parts[2] if len(parts) > 2 else "unknown"
                username = parts[-1] if len(parts) > 0 else "unknown"
                display = f"{username}@{domain}"
            else:
                display = actor_id

            # Add status indicator
            if status == "approved":
                indicator = "✓"
            elif status == "pending":
                indicator = "?"
            elif status == "rejected":
                indicator = "✗"
            else:
                indicator = "·"

            follower_text = f"{indicator} {display}"
            list_view.append(ListItem(Label(follower_text)))

    def on_mount(self) -> None:
        """Load followers when widget is mounted."""
        self.refresh_data()
