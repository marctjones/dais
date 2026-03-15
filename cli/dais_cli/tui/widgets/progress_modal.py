"""Progress indicator modal for long-running operations."""

from textual.app import ComposeResult
from textual.screen import ModalScreen
from textual.widgets import Static, ProgressBar
from textual.containers import Container, Vertical
from textual.binding import Binding


class ProgressModal(ModalScreen[bool]):
    """Modal dialog showing progress for long-running operations.

    Returns True when complete, False if cancelled.

    Example:
        progress = ProgressModal(
            title="Publishing Post",
            steps=["Saving to database", "Delivering to followers", "Queueing activities"]
        )
        await self.app.push_screen(progress)

        # Update progress
        progress.update_step(0, "complete")  # Mark first step complete
        progress.update_step(1, "active")    # Activate second step
        progress.set_progress(0.5)           # Set overall progress to 50%
    """

    BINDINGS = [
        Binding("escape", "cancel", "Cancel", show=True),
    ]

    CSS = """
    ProgressModal {
        align: center middle;
    }

    #progress-dialog {
        background: $surface;
        border: thick $primary;
        padding: 1 2;
        width: 60;
        height: auto;
    }

    #progress-title {
        color: $accent;
        text-style: bold;
        text-align: center;
        padding: 1 0;
    }

    .step-item {
        padding: 0 1;
        height: auto;
    }

    .step-complete {
        color: $success;
    }

    .step-active {
        color: $accent;
        text-style: bold;
    }

    .step-pending {
        color: $text-muted;
    }

    .step-error {
        color: $error;
    }

    #progress-bar {
        margin: 1 0;
    }

    #status-message {
        color: $text-muted;
        text-align: center;
        padding: 1 0;
    }
    """

    def __init__(
        self,
        title: str = "Processing",
        steps: list[str] | None = None,
        *args,
        **kwargs
    ):
        super().__init__(*args, **kwargs)
        self.modal_title = title
        self.steps = steps or []
        self.step_states = ["pending"] * len(self.steps)  # pending, active, complete, error
        self.status_message = ""

    def compose(self) -> ComposeResult:
        """Compose the progress modal UI."""
        with Container(id="progress-dialog"):
            yield Static(self.modal_title, id="progress-title")

            # Progress bar
            yield ProgressBar(total=100, show_eta=False, id="progress-bar")

            # Step list
            with Vertical(id="steps-container"):
                for i, step in enumerate(self.steps):
                    yield Static(f"○ {step}", classes="step-item step-pending", id=f"step-{i}")

            yield Static("", id="status-message")

    def update_step(self, step_index: int, state: str) -> None:
        """Update the state of a step.

        Args:
            step_index: Index of the step (0-based)
            state: One of "pending", "active", "complete", "error"
        """
        if step_index < 0 or step_index >= len(self.steps):
            return

        self.step_states[step_index] = state

        # Update visual representation
        icons = {
            "pending": "○",
            "active": "◉",
            "complete": "✓",
            "error": "✗"
        }

        icon = icons.get(state, "○")
        text = f"{icon} {self.steps[step_index]}"

        step_widget = self.query_one(f"#step-{step_index}", Static)
        step_widget.update(text)

        # Update styling
        step_widget.remove_class("step-pending", "step-active", "step-complete", "step-error")
        step_widget.add_class(f"step-{state}")

    def set_progress(self, percentage: float) -> None:
        """Set the overall progress percentage.

        Args:
            percentage: Progress from 0.0 to 1.0
        """
        progress_bar = self.query_one("#progress-bar", ProgressBar)
        progress_bar.update(progress=percentage * 100)

    def set_status(self, message: str) -> None:
        """Set the status message shown below the progress bar.

        Args:
            message: Status message to display
        """
        self.status_message = message
        status_widget = self.query_one("#status-message", Static)
        status_widget.update(message)

    def mark_complete(self) -> None:
        """Mark all steps as complete and dismiss modal."""
        for i in range(len(self.steps)):
            self.update_step(i, "complete")
        self.set_progress(1.0)
        self.set_status("Complete!")
        self.dismiss(True)

    def mark_error(self, error_message: str, failed_step: int | None = None) -> None:
        """Mark operation as failed.

        Args:
            error_message: Error message to display
            failed_step: Index of the step that failed (optional)
        """
        if failed_step is not None:
            self.update_step(failed_step, "error")

        self.set_status(f"Error: {error_message}")
        self.set_progress(0.0)

    def action_cancel(self) -> None:
        """Handle cancel action."""
        self.dismiss(False)
