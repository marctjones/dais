"""Dashboard screen for dais TUI."""

from textual.app import ComposeResult
from textual.screen import Screen
from textual.containers import Container, Horizontal, Vertical
from textual.widgets import Static, Button
from textual.reactive import reactive

from dais_cli.tui.widgets.stats import StatsWidget
from dais_cli.tui.widgets.post_list import PostListWidget
from dais_cli.tui.widgets.follower_list import FollowerListWidget


class DashboardScreen(Screen):
    """Main dashboard showing overview of server status."""

    CSS = """
    DashboardScreen {
        layout: grid;
        grid-size: 2 3;
        grid-rows: auto 1fr auto;
        padding: 1;
    }

    .dashboard-header {
        column-span: 2;
        height: 3;
        border: solid $primary;
        padding: 1;
    }

    .left-panel {
        height: 100%;
    }

    .right-panel {
        height: 100%;
    }

    .actions {
        column-span: 2;
        height: 5;
        align: center middle;
    }

    Button {
        margin: 0 1;
    }
    """

    BINDINGS = [
        ("r", "refresh", "Refresh"),
        ("escape", "app.pop_screen", "Back"),
    ]

    refresh_count = reactive(0)

    def compose(self) -> ComposeResult:
        """Compose the dashboard layout."""
        with Container(classes="dashboard-header"):
            yield Static(
                "📊 Dashboard - Real-time server overview",
                id="dashboard-title"
            )

        with Vertical(classes="left-panel"):
            yield StatsWidget(id="stats")
            yield FollowerListWidget(id="followers")

        with Vertical(classes="right-panel"):
            yield PostListWidget(id="posts")

        with Horizontal(classes="actions"):
            yield Button("📝 New Post", id="new-post", variant="primary")
            yield Button("👥 Followers", id="view-followers", variant="default")
            yield Button("🔔 Notifications", id="view-notifications", variant="default")
            yield Button("✉️ DMs", id="view-dms", variant="default")
            yield Button("🛡️ Moderation", id="view-moderation", variant="default")
            yield Button("🚫 Blocks", id="view-blocks", variant="default")
            yield Button("🔄 Refresh", id="refresh", variant="default")

    def on_mount(self) -> None:
        """Load data when screen is mounted."""
        self.action_refresh()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "new-post":
            from dais_cli.tui.screens.composer import ComposerScreen
            self.app.push_screen(ComposerScreen())
        elif event.button.id == "view-followers":
            from dais_cli.tui.screens.followers import FollowersScreen
            self.app.push_screen(FollowersScreen())
        elif event.button.id == "view-notifications":
            from dais_cli.tui.screens.notifications import NotificationsScreen
            self.app.push_screen(NotificationsScreen())
        elif event.button.id == "view-dms":
            from dais_cli.tui.screens.direct_messages import DirectMessagesScreen
            self.app.push_screen(DirectMessagesScreen())
        elif event.button.id == "view-moderation":
            from dais_cli.tui.screens.moderation import ModerationScreen
            self.app.push_screen(ModerationScreen())
        elif event.button.id == "view-blocks":
            from dais_cli.tui.screens.blocks import BlocksScreen
            self.app.push_screen(BlocksScreen())
        elif event.button.id == "refresh":
            self.action_refresh()

    def action_refresh(self) -> None:
        """Refresh all data on the dashboard."""
        self.refresh_count += 1

        # Refresh widgets
        stats_widget = self.query_one("#stats", StatsWidget)
        posts_widget = self.query_one("#posts", PostListWidget)
        followers_widget = self.query_one("#followers", FollowerListWidget)

        stats_widget.refresh_data()
        posts_widget.refresh_data()
        followers_widget.refresh_data()

        self.notify(f"Dashboard refreshed", severity="information")
