"""Notifications screen."""

import subprocess
import json
from pathlib import Path
from textual.app import ComposeResult
from textual.screen import Screen
from textual.containers import Container, Horizontal
from textual.widgets import Static, ListView, ListItem, Label, Button
from textual.binding import Binding


class NotificationItem(ListItem):
    """Custom list item for notifications."""

    def __init__(self, notification_id: str, notification_data: dict, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.notification_id = notification_id
        self.notification_data = notification_data


class NotificationsScreen(Screen):
    """Screen for viewing notifications."""

    CSS = """
    NotificationsScreen {
        align: center top;
        padding: 2;
    }

    #notifications-container {
        width: 100%;
        height: 100%;
        border: solid $primary;
        background: $surface;
        padding: 1;
    }

    #notifications-title {
        text-align: center;
        padding-bottom: 1;
        color: $primary;
    }

    #filter-tabs {
        height: 3;
        padding-bottom: 1;
    }

    #notifications-list-view {
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

    .notification-unread {
        background: $accent-darken-1;
    }
    """

    BINDINGS = [
        Binding("r", "refresh", "Refresh", show=True),
        Binding("a", "filter_all", "All", show=True),
        Binding("m", "filter_mentions", "Mentions", show=True),
        Binding("f", "filter_follows", "Follows", show=True),
        Binding("escape", "app.pop_screen", "Back", show=True),
    ]

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.notifications = []
        self.current_filter = "all"

    def compose(self) -> ComposeResult:
        """Compose the notifications UI."""
        with Container(id="notifications-container"):
            yield Static("🔔 **Notifications**", id="notifications-title")

            with Horizontal(id="filter-tabs"):
                yield Button("All", id="filter-all", variant="primary")
                yield Button("Mentions", id="filter-mention", variant="default")
                yield Button("Replies", id="filter-reply", variant="default")
                yield Button("Likes", id="filter-like", variant="default")
                yield Button("Boosts", id="filter-boost", variant="default")
                yield Button("Follows", id="filter-follow", variant="default")

            yield ListView(id="notifications-list-view")

            with Horizontal(classes="actions"):
                yield Button("✓ Mark All Read", id="mark-read-btn", variant="success")
                yield Button("🔄 Refresh", id="refresh-btn", variant="default")
                yield Button("← Back", id="back-btn", variant="default")

    def on_mount(self) -> None:
        """Load notifications when screen is mounted."""
        self.action_refresh()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "refresh-btn":
            self.action_refresh()
        elif event.button.id == "back-btn":
            self.app.pop_screen()
        elif event.button.id == "mark-read-btn":
            self._mark_all_read()
        elif event.button.id.startswith("filter-"):
            filter_type = event.button.id.replace("filter-", "")
            self.current_filter = filter_type
            self._update_filter_buttons()
            self._update_display()

    def _update_filter_buttons(self) -> None:
        """Update button variants based on active filter."""
        filter_buttons = ["filter-all", "filter-mention", "filter-reply", "filter-like", "filter-boost", "filter-follow"]
        for btn_id in filter_buttons:
            button = self.query_one(f"#{btn_id}", Button)
            if btn_id == f"filter-{self.current_filter}":
                button.variant = "primary"
            else:
                button.variant = "default"

    def action_refresh(self) -> None:
        """Refresh notifications from database."""
        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        try:
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 """SELECT id, type, actor_username, actor_display_name, content,
                           post_id, read, created_at
                    FROM notifications
                    ORDER BY created_at DESC
                    LIMIT 100;"""],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )

            if result.returncode == 0:
                data = json.loads(result.stdout)
                if data and len(data) > 0 and "results" in data[0]:
                    self.notifications = data[0]["results"]

        except Exception:
            self.notifications = []

        self._update_display()
        self.notify("Notifications refreshed", severity="information")

    def _update_display(self) -> None:
        """Update the notifications list display with current filter."""
        list_view = self.query_one("#notifications-list-view", ListView)
        list_view.clear()

        # Filter notifications based on current filter
        if self.current_filter == "all":
            filtered = self.notifications
        else:
            filtered = [n for n in self.notifications if n.get("type") == self.current_filter]

        if not filtered:
            list_view.append(ListItem(Label(f"No {self.current_filter} notifications")))
            return

        for notification in filtered:
            notif_id = notification.get("id", "")
            notif_type = notification.get("type", "unknown")
            actor = notification.get("actor_username", "unknown")
            display_name = notification.get("actor_display_name", "")
            content = notification.get("content", "")
            is_read = notification.get("read", 0) == 1
            created = notification.get("created_at", "")[:19]

            # Type-specific icons and messages
            type_icons = {
                "mention": "💬",
                "reply": "↩️",
                "like": "❤️",
                "boost": "🔁",
                "follow": "👤"
            }
            icon = type_icons.get(notif_type, "🔔")

            # Use display name if available, otherwise username
            display = display_name if display_name else actor

            # Format notification text
            if notif_type == "follow":
                text = f"{icon} {display} followed you"
            elif notif_type == "like":
                text = f"{icon} {display} liked your post"
            elif notif_type == "boost":
                text = f"{icon} {display} boosted your post"
            elif notif_type == "reply":
                preview = content[:80] if content else "replied to your post"
                text = f"{icon} {display}: {preview}"
            elif notif_type == "mention":
                preview = content[:80] if content else "mentioned you"
                text = f"{icon} {display}: {preview}"
            else:
                text = f"{icon} {display}: {content[:80]}"

            # Add read indicator
            if not is_read:
                text = f"[bold]{text}[/bold]"

            text += f"\n[dim]{created}[/dim]"

            item = NotificationItem(notif_id, notification, Label(text))
            if not is_read:
                item.add_class("notification-unread")
            list_view.append(item)

    def _mark_all_read(self) -> None:
        """Mark all notifications as read."""
        project_root = Path(__file__).parent.parent.parent.parent.parent
        try:
            result = subprocess.run(
                ["dais", "notifications", "clear"],
                capture_output=True,
                text=True,
                cwd=str(project_root)
            )

            if result.returncode == 0:
                self.notify("✓ All notifications marked as read", severity="information")
                self.action_refresh()
            else:
                error_msg = result.stderr or result.stdout or "Unknown error"
                self.notify(f"Failed: {error_msg[:50]}", severity="error")

        except Exception as e:
            self.notify(f"Error: {str(e)}", severity="error")

    def action_filter_all(self) -> None:
        """Show all notifications."""
        self.current_filter = "all"
        self._update_filter_buttons()
        self._update_display()

    def action_filter_mentions(self) -> None:
        """Show only mentions."""
        self.current_filter = "mention"
        self._update_filter_buttons()
        self._update_display()

    def action_filter_follows(self) -> None:
        """Show only follows."""
        self.current_filter = "follow"
        self._update_filter_buttons()
        self._update_display()
