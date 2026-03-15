"""Theme selector screen for choosing color schemes."""

from textual.app import ComposeResult
from textual.screen import Screen
from textual.widgets import Static, DataTable, Label
from textual.containers import Container, Vertical
from textual.binding import Binding
from rich.text import Text

from dais_cli.tui.themes import THEMES, get_theme_names
from dais_cli.config import Config


class ThemeSelectorScreen(Screen):
    """Theme selector with live preview.

    Allows users to browse and select color themes for the TUI.
    """

    CSS = """
    ThemeSelectorScreen {
        background: $surface;
    }

    #title {
        color: $accent;
        text-style: bold;
        padding: 1;
        text-align: center;
    }

    #description {
        color: $text-muted;
        padding: 0 2 1 2;
        text-align: center;
    }

    #theme-table {
        height: 1fr;
        margin: 1 2;
    }

    #preview-container {
        height: 12;
        margin: 1 2;
        border: solid $primary;
        padding: 1;
    }

    #preview-title {
        color: $accent;
        text-style: bold;
        padding: 0 0 1 0;
    }

    .preview-color {
        padding: 0 1;
    }

    #instructions {
        color: $text-muted;
        text-align: center;
        padding: 1;
    }
    """

    BINDINGS = [
        Binding("enter", "select_theme", "Apply Theme", show=True),
        Binding("escape", "app.pop_screen", "Back", show=True),
        Binding("r", "reset_theme", "Reset to Default", show=True),
    ]

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.selected_theme = "default"

    def compose(self) -> ComposeResult:
        """Compose the theme selector UI."""
        yield Static("🎨 Theme Selector", id="title")
        yield Static(
            "Choose a color theme for your dais TUI experience",
            id="description"
        )

        # Theme table
        table = DataTable(id="theme-table", cursor_type="row")
        table.add_column("Theme", width=20)
        table.add_column("Description", width=50)
        table.add_column("Style", width=15)
        yield table

        # Preview container
        with Container(id="preview-container"):
            yield Static("Preview", id="preview-title")
            with Vertical(id="preview-colors"):
                yield Static("", classes="preview-color", id="preview-primary")
                yield Static("", classes="preview-color", id="preview-accent")
                yield Static("", classes="preview-color", id="preview-success")
                yield Static("", classes="preview-color", id="preview-warning")
                yield Static("", classes="preview-color", id="preview-error")

        yield Static(
            "↑/↓: Navigate | Enter: Apply Theme | R: Reset to Default | Esc: Back",
            id="instructions"
        )

    def on_mount(self) -> None:
        """Load themes when screen mounts."""
        self._load_themes()
        self._load_current_theme()

    def _load_themes(self) -> None:
        """Load available themes into the table."""
        table = self.query_one("#theme-table", DataTable)

        for theme_key, theme in THEMES.items():
            # Determine theme style category
            if "light" in theme_key:
                style = "Light"
            elif any(word in theme_key for word in ["ocean", "nord", "solarized"]):
                style = "Cool"
            elif any(word in theme_key for word in ["forest", "gruvbox"]):
                style = "Warm"
            elif any(word in theme_key for word in ["sunset", "monokai", "dracula"]):
                style = "Vibrant"
            else:
                style = "Neutral"

            table.add_row(theme.name, theme.description, style, key=theme_key)

    def _load_current_theme(self) -> None:
        """Load the currently configured theme."""
        config = Config()
        config.load()
        current_theme = config.get("tui.theme", "default")
        self.selected_theme = current_theme

        # Select current theme in table
        table = self.query_one("#theme-table", DataTable)
        for row_key in table.rows:
            if row_key.value == current_theme:
                table.move_cursor(row=row_key)
                break

        self._update_preview(current_theme)

    def _update_preview(self, theme_key: str) -> None:
        """Update the preview section with theme colors.

        Args:
            theme_key: Key of the theme to preview
        """
        theme = THEMES.get(theme_key)
        if not theme:
            return

        # Update preview colors
        self.query_one("#preview-primary", Static).update(
            f"[{theme.primary}]█████[/] Primary: {theme.primary}"
        )
        self.query_one("#preview-accent", Static).update(
            f"[{theme.accent}]█████[/] Accent: {theme.accent}"
        )
        self.query_one("#preview-success", Static).update(
            f"[{theme.success}]█████[/] Success: {theme.success}"
        )
        self.query_one("#preview-warning", Static).update(
            f"[{theme.warning}]█████[/] Warning: {theme.warning}"
        )
        self.query_one("#preview-error", Static).update(
            f"[{theme.error}]█████[/] Error: {theme.error}"
        )

    def on_data_table_row_highlighted(self, event: DataTable.RowHighlighted) -> None:
        """Handle theme selection change."""
        if event.row_key:
            theme_key = event.row_key.value
            self._update_preview(theme_key)

    def action_select_theme(self) -> None:
        """Apply the selected theme."""
        table = self.query_one("#theme-table", DataTable)
        if table.cursor_row is None:
            return

        row_key = table.get_row_at(table.cursor_row)
        if not row_key:
            return

        theme_key = row_key[0]

        # Save theme to config
        config = Config()
        config.load()
        config.set("tui.theme", theme_key)
        config.save()

        self.notify(
            f"Theme '{THEMES[theme_key].name}' applied! Restart the TUI to see changes.",
            severity="information",
            timeout=5
        )

        self.selected_theme = theme_key

    def action_reset_theme(self) -> None:
        """Reset to default theme."""
        config = Config()
        config.load()
        config.set("tui.theme", "default")
        config.save()

        self.notify("Theme reset to default! Restart the TUI to see changes.", severity="information")
        self.selected_theme = "default"

        # Select default in table
        table = self.query_one("#theme-table", DataTable)
        for row_key in table.rows:
            if row_key.value == "default":
                table.move_cursor(row=row_key)
                break
