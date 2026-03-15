"""Content moderation screen."""

import subprocess
import json
from pathlib import Path
from textual.app import ComposeResult
from textual.screen import Screen
from textual.containers import Container, Horizontal, Vertical
from textual.widgets import Static, ListView, ListItem, Label, Button, Select
from textual.binding import Binding


class ModerationItem(ListItem):
    """Custom list item for moderated replies with action buttons."""

    def __init__(self, reply_id: str, reply_data: dict, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.reply_id = reply_id
        self.reply_data = reply_data


class ModerationScreen(Screen):
    """Screen for reviewing and moderating replies."""

    CSS = """
    ModerationScreen {
        align: center top;
        padding: 2;
    }

    #moderation-container {
        width: 100%;
        height: 100%;
        border: solid $primary;
        background: $surface;
        padding: 1;
    }

    #moderation-title {
        text-align: center;
        padding-bottom: 1;
        color: $primary;
    }

    #filter-tabs {
        height: 3;
        padding-bottom: 1;
    }

    #moderation-list-view {
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

    .score-high {
        color: $error;
    }

    .score-medium {
        color: $warning;
    }

    .score-low {
        color: $success;
    }
    """

    BINDINGS = [
        Binding("r", "refresh", "Refresh", show=True),
        Binding("a", "filter_all", "All", show=True),
        Binding("p", "filter_pending", "Pending", show=True),
        Binding("h", "filter_hidden", "Hidden", show=True),
        Binding("escape", "app.pop_screen", "Back", show=True),
    ]

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.replies = []
        self.current_filter = "pending"

    def compose(self) -> ComposeResult:
        """Compose the moderation UI."""
        with Container(id="moderation-container"):
            yield Static("🛡️ **Content Moderation**", id="moderation-title")

            with Horizontal(id="filter-tabs"):
                yield Button("All", id="filter-all", variant="default")
                yield Button("Pending", id="filter-pending", variant="primary")
                yield Button("Hidden", id="filter-hidden", variant="default")
                yield Button("Approved", id="filter-approved", variant="default")

            yield ListView(id="moderation-list-view")

            with Horizontal(classes="actions"):
                yield Button("✓ Approve", id="approve-btn", variant="success")
                yield Button("✗ Reject", id="reject-btn", variant="error")
                yield Button("🔄 Refresh", id="refresh-btn", variant="default")
                yield Button("← Back", id="back-btn", variant="default")

    def on_mount(self) -> None:
        """Load moderation queue when screen is mounted."""
        self.action_refresh()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "refresh-btn":
            self.action_refresh()
        elif event.button.id == "back-btn":
            self.app.pop_screen()
        elif event.button.id == "approve-btn":
            self._approve_selected()
        elif event.button.id == "reject-btn":
            self._reject_selected()
        elif event.button.id.startswith("filter-"):
            filter_type = event.button.id.replace("filter-", "")
            self.current_filter = filter_type
            self._update_filter_buttons()
            self._update_display()

    def _update_filter_buttons(self) -> None:
        """Update button variants based on active filter."""
        for btn_id in ["filter-all", "filter-pending", "filter-hidden", "filter-approved"]:
            button = self.query_one(f"#{btn_id}", Button)
            if btn_id == f"filter-{self.current_filter}":
                button.variant = "primary"
            else:
                button.variant = "default"

    def action_refresh(self) -> None:
        """Refresh moderation queue from database."""
        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        try:
            # Query replies with moderation data
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 """SELECT r.id, r.post_id, r.actor_username, r.content, r.published_at,
                           r.moderation_status, r.moderation_score, r.moderation_flags, r.hidden
                    FROM replies r
                    ORDER BY r.published_at DESC;"""],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )

            if result.returncode == 0:
                data = json.loads(result.stdout)
                if data and len(data) > 0 and "results" in data[0]:
                    self.replies = data[0]["results"]

        except Exception:
            self.replies = []

        self._update_display()
        self.notify("Moderation queue refreshed", severity="information")

    def _update_display(self) -> None:
        """Update the moderation list display with current filter."""
        list_view = self.query_one("#moderation-list-view", ListView)
        list_view.clear()

        # Filter replies based on current filter
        if self.current_filter == "all":
            filtered = self.replies
        elif self.current_filter == "pending":
            filtered = [r for r in self.replies if r.get("moderation_status") == "pending"]
        elif self.current_filter == "hidden":
            filtered = [r for r in self.replies if r.get("hidden") == 1 or r.get("moderation_status") == "hidden"]
        elif self.current_filter == "approved":
            filtered = [r for r in self.replies if r.get("moderation_status") == "approved"]
        else:
            filtered = self.replies

        if not filtered:
            list_view.append(ListItem(Label(f"No {self.current_filter} replies")))
            return

        for reply in filtered:
            reply_id = reply.get("id", "")
            actor = reply.get("actor_username", "unknown")
            content = reply.get("content", "")[:100]  # Truncate
            status = reply.get("moderation_status", "unknown")
            score = reply.get("moderation_score", 0.0) or 0.0
            flags = reply.get("moderation_flags", "")

            # Color code by score
            if score > 0.7:
                score_class = "score-high"
                score_indicator = "🔴"
            elif score > 0.4:
                score_class = "score-medium"
                score_indicator = "🟡"
            else:
                score_class = "score-low"
                score_indicator = "🟢"

            # Status indicator
            status_indicators = {
                "approved": "✓",
                "pending": "?",
                "hidden": "👁",
                "rejected": "✗"
            }
            status_indicator = status_indicators.get(status, "·")

            reply_text = f"{status_indicator} {score_indicator} {actor}: {content}\n[dim]Score: {score:.2f} • Status: {status}"
            if flags:
                reply_text += f" • Flags: {flags}"
            reply_text += "[/dim]"

            list_view.append(ModerationItem(reply_id, reply, Label(reply_text)))

    def _approve_selected(self) -> None:
        """Approve the selected reply."""
        list_view = self.query_one("#moderation-list-view", ListView)
        if list_view.index is None:
            self.notify("No reply selected", severity="warning")
            return

        selected = list(list_view.children)[list_view.index]
        if not isinstance(selected, ModerationItem):
            self.notify("Invalid selection", severity="error")
            return

        project_root = Path(__file__).parent.parent.parent.parent.parent
        try:
            result = subprocess.run(
                ["dais", "moderation", "approve", selected.reply_id],
                capture_output=True,
                text=True,
                cwd=str(project_root)
            )

            if result.returncode == 0:
                self.notify(f"✓ Reply approved", severity="information")
                self.action_refresh()
            else:
                error_msg = result.stderr or result.stdout or "Unknown error"
                self.notify(f"Failed to approve: {error_msg[:50]}", severity="error")

        except Exception as e:
            self.notify(f"Error: {str(e)}", severity="error")

    def _reject_selected(self) -> None:
        """Reject and hide the selected reply."""
        list_view = self.query_one("#moderation-list-view", ListView)
        if list_view.index is None:
            self.notify("No reply selected", severity="warning")
            return

        selected = list(list_view.children)[list_view.index]
        if not isinstance(selected, ModerationItem):
            self.notify("Invalid selection", severity="error")
            return

        project_root = Path(__file__).parent.parent.parent.parent.parent
        try:
            result = subprocess.run(
                ["dais", "moderation", "reject", selected.reply_id],
                capture_output=True,
                text=True,
                cwd=str(project_root)
            )

            if result.returncode == 0:
                self.notify(f"✗ Reply rejected and hidden", severity="information")
                self.action_refresh()
            else:
                error_msg = result.stderr or result.stdout or "Unknown error"
                self.notify(f"Failed to reject: {error_msg[:50]}", severity="error")

        except Exception as e:
            self.notify(f"Error: {str(e)}", severity="error")

    def action_filter_all(self) -> None:
        """Show all replies."""
        self.current_filter = "all"
        self._update_filter_buttons()
        self._update_display()

    def action_filter_pending(self) -> None:
        """Show only pending replies."""
        self.current_filter = "pending"
        self._update_filter_buttons()
        self._update_display()

    def action_filter_hidden(self) -> None:
        """Show only hidden replies."""
        self.current_filter = "hidden"
        self._update_filter_buttons()
        self._update_display()
