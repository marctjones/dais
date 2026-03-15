"""Block management screen."""

import subprocess
import json
from pathlib import Path
from textual.app import ComposeResult
from textual.screen import Screen
from textual.containers import Container, Horizontal, Vertical
from textual.widgets import Static, ListView, ListItem, Label, Button, Input
from textual.binding import Binding


class BlockItem(ListItem):
    """Custom list item for blocked actors/domains."""

    def __init__(self, block_id: str, block_data: dict, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.block_id = block_id
        self.block_data = block_data


class BlocksScreen(Screen):
    """Screen for managing blocked users and domains."""

    CSS = """
    BlocksScreen {
        align: center top;
        padding: 2;
    }

    #blocks-container {
        width: 100%;
        height: 100%;
        border: solid $primary;
        background: $surface;
        padding: 1;
    }

    #blocks-title {
        text-align: center;
        padding-bottom: 1;
        color: $primary;
    }

    #add-block-section {
        height: auto;
        padding: 1;
        border: solid $accent;
        margin-bottom: 1;
    }

    #block-input {
        width: 1fr;
        margin-right: 1;
    }

    #blocks-list-view {
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
        Binding("d", "delete_block", "Delete", show=True),
        Binding("escape", "app.pop_screen", "Back", show=True),
    ]

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.blocks = []

    def compose(self) -> ComposeResult:
        """Compose the block management UI."""
        with Container(id="blocks-container"):
            yield Static("🚫 **Block Management**", id="blocks-title")

            with Container(id="add-block-section"):
                yield Static("Add Block:")
                with Horizontal():
                    yield Input(
                        placeholder="@user@domain or domain.com",
                        id="block-input"
                    )
                    yield Button("🚫 Block", id="add-block-btn", variant="error")

            yield ListView(id="blocks-list-view")

            with Horizontal(classes="actions"):
                yield Button("🗑️ Unblock", id="unblock-btn", variant="warning")
                yield Button("🔄 Refresh", id="refresh-btn", variant="default")
                yield Button("← Back", id="back-btn", variant="default")

    def on_mount(self) -> None:
        """Load blocks when screen is mounted."""
        self.action_refresh()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "refresh-btn":
            self.action_refresh()
        elif event.button.id == "back-btn":
            self.app.pop_screen()
        elif event.button.id == "add-block-btn":
            self._add_block()
        elif event.button.id == "unblock-btn":
            self._unblock_selected()

    def on_input_submitted(self, event: Input.Submitted) -> None:
        """Handle Enter key in input field."""
        if event.input.id == "block-input":
            self._add_block()

    def action_refresh(self) -> None:
        """Refresh block list from database."""
        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        try:
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 """SELECT id, actor_id, blocked_domain, reason, created_at
                    FROM blocks
                    ORDER BY created_at DESC;"""],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )

            if result.returncode == 0:
                data = json.loads(result.stdout)
                if data and len(data) > 0 and "results" in data[0]:
                    self.blocks = data[0]["results"]

        except Exception:
            self.blocks = []

        self._update_display()
        self.notify("Block list refreshed", severity="information")

    def _update_display(self) -> None:
        """Update the block list display."""
        list_view = self.query_one("#blocks-list-view", ListView)
        list_view.clear()

        if not self.blocks:
            list_view.append(ListItem(Label("No blocks configured")))
            return

        for block in self.blocks:
            block_id = block.get("id", "")
            actor_id = block.get("actor_id", "")
            domain = block.get("blocked_domain", "")
            reason = block.get("reason", "")
            created = block.get("created_at", "")[:19]

            # Determine what's blocked
            if domain:
                target = f"🌐 {domain} (entire domain)"
            elif actor_id:
                # Extract username from actor ID
                if "/" in actor_id:
                    parts = actor_id.split("/")
                    domain_part = parts[2] if len(parts) > 2 else "unknown"
                    username = parts[-1] if len(parts) > 0 else "unknown"
                    target = f"👤 {username}@{domain_part}"
                else:
                    target = f"👤 {actor_id}"
            else:
                target = "Unknown"

            block_text = f"🚫 {target}"
            if reason:
                block_text += f"\n[dim]Reason: {reason[:60]} • {created}[/dim]"
            else:
                block_text += f"\n[dim]Blocked at: {created}[/dim]"

            list_view.append(BlockItem(block_id, block, Label(block_text)))

    def _add_block(self) -> None:
        """Add a new block."""
        input_widget = self.query_one("#block-input", Input)
        target = input_widget.value.strip()

        if not target:
            self.notify("Please enter a user or domain to block", severity="warning")
            return

        project_root = Path(__file__).parent.parent.parent.parent.parent
        try:
            result = subprocess.run(
                ["dais", "block", "add", target],
                capture_output=True,
                text=True,
                cwd=str(project_root)
            )

            if result.returncode == 0:
                self.notify(f"✓ Blocked {target}", severity="information")
                input_widget.value = ""
                self.action_refresh()
            else:
                error_msg = result.stderr or result.stdout or "Unknown error"
                self.notify(f"Failed to block: {error_msg[:50]}", severity="error")

        except Exception as e:
            self.notify(f"Error: {str(e)}", severity="error")

    def _unblock_selected(self) -> None:
        """Unblock the selected item."""
        list_view = self.query_one("#blocks-list-view", ListView)
        if list_view.index is None:
            self.notify("No block selected", severity="warning")
            return

        selected = list(list_view.children)[list_view.index]
        if not isinstance(selected, BlockItem):
            self.notify("Invalid selection", severity="error")
            return

        # Determine what to unblock
        actor_id = selected.block_data.get("actor_id", "")
        domain = selected.block_data.get("blocked_domain", "")
        target = domain if domain else actor_id

        if not target:
            self.notify("Invalid block data", severity="error")
            return

        project_root = Path(__file__).parent.parent.parent.parent.parent
        try:
            result = subprocess.run(
                ["dais", "block", "remove", target],
                capture_output=True,
                text=True,
                cwd=str(project_root)
            )

            if result.returncode == 0:
                self.notify(f"✓ Unblocked {target}", severity="information")
                self.action_refresh()
            else:
                error_msg = result.stderr or result.stdout or "Unknown error"
                self.notify(f"Failed to unblock: {error_msg[:50]}", severity="error")

        except Exception as e:
            self.notify(f"Error: {str(e)}", severity="error")

    def action_delete_block(self) -> None:
        """Delete the selected block (alias for unblock)."""
        self._unblock_selected()
