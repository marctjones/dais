"""Main TUI application for dais."""

from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.widgets import Header, Footer
from rich.console import Console

from dais_cli.config import Config
from dais_cli.tui.screens.dashboard import DashboardScreen
from dais_cli.tui.themes import get_theme

console = Console()


class DaisApp(App):
    """Terminal UI for dais - Interactive ActivityPub server management."""

    CSS = """
    Screen {
        background: $surface;
    }

    Header {
        background: $primary;
        color: $text;
    }

    Footer {
        background: $primary-darken-2;
    }
    """

    TITLE = "dais - ActivityPub Server"
    SUB_TITLE = "Interactive Terminal UI"

    BINDINGS = [
        Binding("q", "quit", "Quit", show=True),
        Binding("d", "show_dashboard", "Dashboard", show=True),
        Binding("n", "new_post", "New Post", show=True),
        Binding("f", "show_followers", "Followers", show=True),
        Binding("m", "show_moderation", "Moderation", show=True),
        Binding("b", "show_blocks", "Blocks", show=True),
        Binding("i", "show_notifications", "Notifications", show=True),
        Binding("x", "show_dms", "DMs", show=True),
        Binding("a", "show_analytics", "Analytics", show=True),
        Binding("t", "show_themes", "Themes", show=True),
        Binding("?", "help", "Help", show=True),
    ]

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.config = Config()
        self.config.load()

        # Get user identity from config
        self.username = self.config.get("server.username", "social")
        self.domain = self.config.get("server.domain", "dais.social")
        self.identity = f"@{self.username}@{self.domain}"

        # Load theme
        theme_name = self.config.get("tui.theme", "default")
        try:
            theme = get_theme(theme_name)
            # Apply theme CSS variables
            self.CSS += theme.to_css_variables()
        except KeyError:
            # Fall back to default theme if configured theme not found
            theme = get_theme("default")
            self.CSS += theme.to_css_variables()

    def compose(self) -> ComposeResult:
        """Create child widgets for the app."""
        yield Header()
        yield Footer()

    def on_mount(self) -> None:
        """Called when app is mounted - show dashboard."""
        self.title = f"dais - {self.identity}"
        self.push_screen(DashboardScreen())

    def action_show_dashboard(self) -> None:
        """Show the dashboard screen."""
        self.push_screen(DashboardScreen())

    def action_new_post(self) -> None:
        """Show the post composer."""
        from dais_cli.tui.screens.composer import ComposerScreen
        self.push_screen(ComposerScreen())

    def action_show_followers(self) -> None:
        """Show the followers management screen."""
        from dais_cli.tui.screens.followers import FollowersScreen
        self.push_screen(FollowersScreen())

    def action_show_moderation(self) -> None:
        """Show the moderation screen."""
        from dais_cli.tui.screens.moderation import ModerationScreen
        self.push_screen(ModerationScreen())

    def action_show_blocks(self) -> None:
        """Show the block management screen."""
        from dais_cli.tui.screens.blocks import BlocksScreen
        self.push_screen(BlocksScreen())

    def action_show_notifications(self) -> None:
        """Show the notifications screen."""
        from dais_cli.tui.screens.notifications import NotificationsScreen
        self.push_screen(NotificationsScreen())

    def action_show_dms(self) -> None:
        """Show the direct messages screen."""
        from dais_cli.tui.screens.direct_messages import DirectMessagesScreen
        self.push_screen(DirectMessagesScreen())

    def action_show_analytics(self) -> None:
        """Show the analytics dashboard screen."""
        from dais_cli.tui.screens.analytics import AnalyticsScreen
        self.push_screen(AnalyticsScreen())

    def action_show_themes(self) -> None:
        """Show the theme selector screen."""
        from dais_cli.tui.screens.theme_selector import ThemeSelectorScreen
        self.push_screen(ThemeSelectorScreen())

    def action_help(self) -> None:
        """Show help screen."""
        help_text = """
        🔑 Keyboard Shortcuts:

        d - Dashboard
        n - New Post
        f - Followers
        m - Moderation
        b - Blocks
        i - Notifications
        x - Direct Messages
        a - Analytics
        t - Themes
        q - Quit

        📡 Protocol Support:
        This TUI works with both ActivityPub and AT Protocol/Bluesky.
        When creating posts, you can choose which protocol(s) to use.

        🎨 Customization:
        Press 't' to change the color theme. Choose from 10+ themes
        including Ocean, Forest, Sunset, Nord, Gruvbox, and more!
        """
        self.notify(help_text, severity="information", timeout=10)


def run_tui():
    """Entry point for running the TUI."""
    app = DaisApp()
    app.run()
