"""Post composer screen for creating new posts."""

import subprocess
from pathlib import Path
from textual.app import ComposeResult
from textual.screen import Screen
from textual.containers import Container, Horizontal
from textual.widgets import Static, TextArea, Button, Select
from textual.binding import Binding


class ComposerScreen(Screen):
    """Screen for composing and publishing new posts."""

    CSS = """
    ComposerScreen {
        align: center middle;
    }

    #composer-container {
        width: 80;
        height: auto;
        border: thick $primary;
        background: $surface;
        padding: 2;
    }

    #composer-title {
        text-align: center;
        padding-bottom: 1;
        color: $primary;
    }

    #post-input {
        height: 10;
        margin-bottom: 1;
    }

    .composer-row {
        height: auto;
        margin-top: 1;
    }

    .actions {
        align: center middle;
        padding-top: 1;
    }

    Button {
        margin: 0 1;
    }
    """

    BINDINGS = [
        Binding("ctrl+s", "publish_post", "Publish", show=True),
        Binding("escape", "app.pop_screen", "Cancel", show=True),
    ]

    def compose(self) -> ComposeResult:
        """Compose the post composer UI."""
        with Container(id="composer-container"):
            yield Static("📝 **New Post**", id="composer-title")

            yield TextArea(
                "",
                id="post-input",
                language="markdown"
            )

            with Container(classes="composer-row"):
                yield Static("Visibility:")
                yield Select(
                    [
                        ("Public", "public"),
                        ("Unlisted", "unlisted"),
                        ("Followers Only", "followers"),
                        ("Direct Message", "direct"),
                    ],
                    value="public",
                    id="visibility-select"
                )

            with Container(classes="composer-row"):
                yield Static("Protocol:")
                yield Select(
                    [
                        ("Both (ActivityPub + AT)", "both"),
                        ("ActivityPub Only", "activitypub"),
                        ("AT Protocol/Bluesky Only", "atproto"),
                    ],
                    value="both",
                    id="protocol-select"
                )

            with Horizontal(classes="actions"):
                yield Button("📤 Publish", id="publish-btn", variant="primary")
                yield Button("❌ Cancel", id="cancel-btn", variant="default")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "publish-btn":
            self.action_publish_post()
        elif event.button.id == "cancel-btn":
            self.app.pop_screen()

    def action_publish_post(self) -> None:
        """Publish the post using dais CLI."""
        text_area = self.query_one("#post-input", TextArea)
        visibility_select = self.query_one("#visibility-select", Select)
        protocol_select = self.query_one("#protocol-select", Select)

        content = text_area.text.strip()

        if not content:
            self.notify("Post content cannot be empty!", severity="error")
            return

        visibility = visibility_select.value
        protocol = protocol_select.value

        # Call dais post create command
        project_root = Path(__file__).parent.parent.parent.parent.parent

        try:
            result = subprocess.run(
                ["dais", "post", "create", content, "--visibility", visibility, "--protocol", protocol],
                capture_output=True,
                text=True,
                cwd=str(project_root)
            )

            if result.returncode == 0:
                self.notify("✓ Post published successfully!", severity="information")
                # Clear the input
                text_area.text = ""
                # Go back to dashboard after a moment
                self.app.pop_screen()
            else:
                error_msg = result.stderr or result.stdout or "Unknown error"
                self.notify(f"Failed to publish: {error_msg[:100]}", severity="error")

        except Exception as e:
            self.notify(f"Error: {str(e)}", severity="error")
