"""Enhanced error dialog with details and copy support."""

from textual.app import ComposeResult
from textual.screen import ModalScreen
from textual.widgets import Static, Button, TextArea
from textual.containers import Container, Horizontal, Vertical, ScrollableContainer
from textual.binding import Binding

try:
    import pyperclip
    CLIPBOARD_AVAILABLE = True
except ImportError:
    CLIPBOARD_AVAILABLE = False


class ErrorDialog(ModalScreen[None]):
    """Enhanced error dialog for displaying errors with details.

    Shows error message, optional details, and allows copying error info.

    Example:
        dialog = ErrorDialog(
            title="Failed to Publish Post",
            message="Could not connect to server",
            details="Connection timeout after 30 seconds\\nServer: https://example.com",
            error_type="NetworkError"
        )
        await self.app.push_screen(dialog)
    """

    BINDINGS = [
        Binding("escape", "dismiss", "Close", show=True),
        Binding("c", "copy_error", "Copy Error", show=True),
        Binding("d", "toggle_details", "Toggle Details", show=True),
    ]

    CSS = """
    ErrorDialog {
        align: center middle;
    }

    #error-dialog {
        background: $surface;
        border: thick $error;
        padding: 1 2;
        width: 70;
        height: auto;
        max-height: 80%;
    }

    #error-title {
        color: $error;
        text-style: bold;
        text-align: center;
        padding: 1 0;
    }

    #error-message {
        color: $text;
        padding: 1;
        text-align: center;
    }

    #error-type {
        color: $warning;
        text-style: italic;
        text-align: center;
        padding: 0 0 1 0;
    }

    #details-container {
        display: none;
        margin: 1 0;
        border: solid $primary;
        padding: 1;
        height: auto;
        max-height: 20;
    }

    #details-container.visible {
        display: block;
    }

    #details-content {
        color: $text-muted;
        height: auto;
    }

    #button-container {
        align: center middle;
        height: auto;
        margin: 1 0;
    }

    .error-button {
        margin: 0 1;
    }

    #copy-status {
        color: $success;
        text-align: center;
        height: auto;
        padding: 1 0;
    }
    """

    def __init__(
        self,
        title: str = "Error",
        message: str = "An error occurred",
        details: str | None = None,
        error_type: str | None = None,
        *args,
        **kwargs
    ):
        super().__init__(*args, **kwargs)
        self.error_title = title
        self.error_message = message
        self.error_details = details or ""
        self.error_type = error_type or ""
        self.details_visible = False

    def compose(self) -> ComposeResult:
        """Compose the error dialog UI."""
        with Container(id="error-dialog"):
            yield Static(f"✗ {self.error_title}", id="error-title")

            if self.error_type:
                yield Static(f"[{self.error_type}]", id="error-type")

            yield Static(self.error_message, id="error-message")

            # Details section (hidden by default)
            with ScrollableContainer(id="details-container"):
                yield Static(self.error_details, id="details-content")

            # Buttons
            with Horizontal(id="button-container"):
                if self.error_details:
                    yield Button("Show Details", id="toggle-details-btn", classes="error-button")
                yield Button("Copy Error", id="copy-btn", classes="error-button")
                yield Button("Close", id="close-btn", classes="error-button", variant="primary")

            yield Static("", id="copy-status")

    def action_dismiss(self) -> None:
        """Close the error dialog."""
        self.dismiss()

    def action_toggle_details(self) -> None:
        """Toggle visibility of error details."""
        if not self.error_details:
            return

        self.details_visible = not self.details_visible
        details_container = self.query_one("#details-container")
        toggle_btn = self.query_one("#toggle-details-btn", Button)

        if self.details_visible:
            details_container.add_class("visible")
            toggle_btn.label = "Hide Details"
        else:
            details_container.remove_class("visible")
            toggle_btn.label = "Show Details"

    def action_copy_error(self) -> None:
        """Copy error information to clipboard."""
        error_text = f"""Error: {self.error_title}
Type: {self.error_type or 'Unknown'}
Message: {self.error_message}

Details:
{self.error_details}
"""

        copy_status = self.query_one("#copy-status", Static)

        if not CLIPBOARD_AVAILABLE:
            copy_status.update("⚠ Clipboard support not available (install pyperclip)")
            return

        try:
            pyperclip.copy(error_text)
            copy_status.update("✓ Error details copied to clipboard")

            # Clear status after 3 seconds
            self.set_timer(3.0, lambda: copy_status.update(""))
        except Exception as e:
            copy_status.update(f"⚠ Failed to copy: {str(e)}")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        button_id = event.button.id

        if button_id == "close-btn":
            self.action_dismiss()
        elif button_id == "copy-btn":
            self.action_copy_error()
        elif button_id == "toggle-details-btn":
            self.action_toggle_details()
