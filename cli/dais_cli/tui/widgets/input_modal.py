"""Modal dialog for text input in Textual TUI."""

from textual.app import ComposeResult
from textual.containers import Container, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, Static
from textual.binding import Binding


class InputModal(ModalScreen[str]):
    """Modal dialog for text input.

    Returns the input text when submitted, or None when cancelled.
    """

    CSS = """
    InputModal {
        align: center middle;
    }

    #modal-container {
        width: 60;
        height: auto;
        border: thick $accent;
        background: $surface;
        padding: 1 2;
    }

    #modal-title {
        text-align: center;
        text-style: bold;
        color: $accent;
        margin-bottom: 1;
    }

    #modal-description {
        text-align: center;
        color: $text-muted;
        margin-bottom: 1;
    }

    #modal-input {
        margin: 1 0;
    }

    #button-container {
        width: 100%;
        height: auto;
        layout: horizontal;
        align: center middle;
        margin-top: 1;
    }

    Button {
        margin: 0 1;
    }
    """

    BINDINGS = [
        Binding("escape", "cancel", "Cancel", show=True),
        Binding("ctrl+s", "submit", "Submit", show=True),
    ]

    def __init__(
        self,
        title: str = "Input",
        description: str = "",
        placeholder: str = "",
        default_value: str = "",
        *args,
        **kwargs
    ):
        """Initialize the input modal.

        Args:
            title: Modal title
            description: Description text shown below title
            placeholder: Placeholder text for input field
            default_value: Default input value
        """
        super().__init__(*args, **kwargs)
        self.modal_title = title
        self.modal_description = description
        self.modal_placeholder = placeholder
        self.modal_default = default_value

    def compose(self) -> ComposeResult:
        """Compose the modal UI."""
        with Container(id="modal-container"):
            yield Static(self.modal_title, id="modal-title")
            if self.modal_description:
                yield Static(self.modal_description, id="modal-description")

            yield Input(
                placeholder=self.modal_placeholder,
                value=self.modal_default,
                id="modal-input"
            )

            with Container(id="button-container"):
                yield Button("Submit", variant="primary", id="submit-btn")
                yield Button("Cancel", variant="default", id="cancel-btn")

    def on_mount(self) -> None:
        """Focus the input field when modal opens."""
        self.query_one("#modal-input", Input).focus()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "submit-btn":
            self.action_submit()
        elif event.button.id == "cancel-btn":
            self.action_cancel()

    def on_input_submitted(self, event: Input.Submitted) -> None:
        """Handle Enter key in input field."""
        self.action_submit()

    def action_submit(self) -> None:
        """Submit the input value."""
        input_widget = self.query_one("#modal-input", Input)
        value = input_widget.value.strip()
        self.dismiss(value if value else None)

    def action_cancel(self) -> None:
        """Cancel the modal without submitting."""
        self.dismiss(None)
