"""Followers management screen."""

import subprocess
import json
from pathlib import Path
from textual.app import ComposeResult
from textual.screen import Screen
from textual.containers import Container, Horizontal, Vertical
from textual.widgets import Static, ListView, ListItem, Label, Button
from textual.binding import Binding


class FollowerItem(ListItem):
    """Custom list item for followers with action buttons."""

    def __init__(self, actor_id: str, status: str, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.actor_id = actor_id
        self.status = status


class FollowersScreen(Screen):
    """Screen for managing followers and follow requests."""

    CSS = """
    FollowersScreen {
        align: center top;
        padding: 2;
    }

    #followers-container {
        width: 100%;
        height: 100%;
        border: solid $primary;
        background: $surface;
        padding: 1;
    }

    #followers-title {
        text-align: center;
        padding-bottom: 1;
        color: $primary;
    }

    #filter-tabs {
        height: 3;
        padding-bottom: 1;
    }

    #follower-list-view {
        height: 1fr;
    }

    .actions {
        height: auto;
        align: center middle;
        padding-top: 1;
    }

    Button {
        margin: 0 1;
    }
    """

    BINDINGS = [
        Binding("r", "refresh", "Refresh", show=True),
        Binding("a", "filter_approved", "Approved", show=True),
        Binding("p", "filter_pending", "Pending", show=True),
        Binding("escape", "app.pop_screen", "Back", show=True),
    ]

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.followers = []
        self.current_filter = "all"

    def compose(self) -> ComposeResult:
        """Compose the followers management UI."""
        with Container(id="followers-container"):
            yield Static("👥 **Followers Management**", id="followers-title")

            with Horizontal(id="filter-tabs"):
                yield Button("All", id="filter-all", variant="primary")
                yield Button("Approved", id="filter-approved", variant="default")
                yield Button("Pending", id="filter-pending", variant="default")
                yield Button("Rejected", id="filter-rejected", variant="default")

            yield ListView(id="follower-list-view")

            with Horizontal(classes="actions"):
                yield Button("🔄 Refresh", id="refresh-btn", variant="default")
                yield Button("← Back", id="back-btn", variant="default")

    def on_mount(self) -> None:
        """Load followers when screen is mounted."""
        self.action_refresh()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "refresh-btn":
            self.action_refresh()
        elif event.button.id == "back-btn":
            self.app.pop_screen()
        elif event.button.id.startswith("filter-"):
            filter_type = event.button.id.replace("filter-", "")
            self.current_filter = filter_type
            self._update_filter_buttons()
            self._update_display()

    def _update_filter_buttons(self) -> None:
        """Update button variants based on active filter."""
        for btn_id in ["filter-all", "filter-approved", "filter-pending", "filter-rejected"]:
            button = self.query_one(f"#{btn_id}", Button)
            if btn_id == f"filter-{self.current_filter}":
                button.variant = "primary"
            else:
                button.variant = "default"

    def action_refresh(self) -> None:
        """Refresh follower list from database."""
        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        try:
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 "SELECT follower_actor_id, status, created_at FROM followers ORDER BY created_at DESC;"],
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
        self.notify("Followers list refreshed", severity="information")

    def _update_display(self) -> None:
        """Update the follower list display with current filter."""
        list_view = self.query_one("#follower-list-view", ListView)
        list_view.clear()

        # Filter followers based on current filter
        if self.current_filter == "all":
            filtered = self.followers
        else:
            filtered = [f for f in self.followers if f.get("status") == self.current_filter]

        if not filtered:
            list_view.append(ListItem(Label(f"No {self.current_filter} followers")))
            return

        for follower in filtered:
            actor_id = follower.get("follower_actor_id", "")
            status = follower.get("status", "unknown")
            created = follower.get("created_at", "")[:19]

            # Extract username from actor ID
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

            follower_text = f"{indicator} {display}\n[dim]{status} • {created}[/dim]"
            list_view.append(FollowerItem(actor_id, status, Label(follower_text)))

    def action_filter_approved(self) -> None:
        """Show only approved followers."""
        self.current_filter = "approved"
        self._update_filter_buttons()
        self._update_display()
