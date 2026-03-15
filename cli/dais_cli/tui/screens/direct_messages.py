"""Direct messages screen."""

import subprocess
import json
from pathlib import Path
from textual.app import ComposeResult
from textual.screen import Screen
from textual.containers import Container, Horizontal, Vertical, VerticalScroll
from textual.widgets import Static, ListView, ListItem, Label, Button, Input, TextArea
from textual.binding import Binding


class ConversationItem(ListItem):
    """Custom list item for conversations."""

    def __init__(self, conversation_id: str, conversation_data: dict, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.conversation_id = conversation_id
        self.conversation_data = conversation_data


class DirectMessagesScreen(Screen):
    """Screen for managing direct messages."""

    CSS = """
    DirectMessagesScreen {
        align: center top;
        padding: 2;
    }

    #dm-container {
        width: 100%;
        height: 100%;
        border: solid $primary;
        background: $surface;
        padding: 1;
    }

    #dm-title {
        text-align: center;
        padding-bottom: 1;
        color: $primary;
    }

    #dm-layout {
        layout: horizontal;
        height: 1fr;
    }

    #conversations-panel {
        width: 40%;
        border-right: solid $accent;
        padding-right: 1;
    }

    #messages-panel {
        width: 60%;
        padding-left: 1;
    }

    #conversations-list {
        height: 1fr;
    }

    #messages-scroll {
        height: 1fr;
        border: solid $accent;
        padding: 1;
        margin-bottom: 1;
    }

    #compose-section {
        height: auto;
    }

    #message-input {
        height: 5;
        margin-bottom: 1;
    }

    .actions {
        height: auto;
        align: center middle;
        padding-top: 1;
    }

    Button {
        margin: 0 1;
    }

    .message-sent {
        background: $primary-darken-2;
        padding: 1;
        margin: 1 0;
        border-radius: 1;
    }

    .message-received {
        background: $accent-darken-2;
        padding: 1;
        margin: 1 0;
        border-radius: 1;
    }
    """

    BINDINGS = [
        Binding("r", "refresh", "Refresh", show=True),
        Binding("n", "new_conversation", "New DM", show=True),
        Binding("b", "switch_to_bluesky", "Bluesky Chats", show=True),
        Binding("escape", "app.pop_screen", "Back", show=True),
    ]

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.conversations = []
        self.current_conversation = None
        self.messages = []

    def compose(self) -> ComposeResult:
        """Compose the DM UI."""
        with Container(id="dm-container"):
            yield Static("✉️ **Direct Messages (ActivityPub)**", id="dm-title")

            with Horizontal(id="dm-layout"):
                # Left panel: Conversations list
                with Vertical(id="conversations-panel"):
                    yield Static("Conversations:", classes="panel-header")
                    yield ListView(id="conversations-list")

                # Right panel: Messages view and compose
                with Vertical(id="messages-panel"):
                    yield Static("Select a conversation", id="conversation-title")
                    with VerticalScroll(id="messages-scroll"):
                        yield Static("", id="messages-content")

                    with Container(id="compose-section"):
                        yield TextArea("", id="message-input", language="markdown")
                        with Horizontal():
                            yield Button("📤 Send", id="send-btn", variant="primary")
                            yield Button("🗑️ Clear", id="clear-btn", variant="default")

            with Horizontal(classes="actions"):
                yield Button("✉️ New DM", id="new-dm-btn", variant="success")
                yield Button("🦋 Bluesky Chats", id="switch-protocol-btn", variant="default")
                yield Button("🔄 Refresh", id="refresh-btn", variant="default")
                yield Button("← Back", id="back-btn", variant="default")

    def on_mount(self) -> None:
        """Load conversations when screen is mounted."""
        self.action_refresh()

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "refresh-btn":
            self.action_refresh()
        elif event.button.id == "back-btn":
            self.app.pop_screen()
        elif event.button.id == "send-btn":
            self._send_message()
        elif event.button.id == "clear-btn":
            self.query_one("#message-input", TextArea).text = ""
        elif event.button.id == "new-dm-btn":
            self._new_conversation()
        elif event.button.id == "switch-protocol-btn":
            self.action_switch_to_bluesky()

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        """Handle conversation selection."""
        if event.list_view.id == "conversations-list" and isinstance(event.item, ConversationItem):
            self._load_conversation(event.item.conversation_id, event.item.conversation_data)

    def action_refresh(self) -> None:
        """Refresh conversations list from database."""
        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        try:
            # Query conversations with most recent message time
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 """SELECT c.id, c.participants, c.last_message_at
                    FROM conversations c
                    ORDER BY c.last_message_at DESC;"""],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )

            if result.returncode == 0:
                data = json.loads(result.stdout)
                if data and len(data) > 0 and "results" in data[0]:
                    self.conversations = data[0]["results"]

        except Exception:
            self.conversations = []

        self._update_conversations_display()
        self.notify("Conversations refreshed", severity="information")

    def _update_conversations_display(self) -> None:
        """Update the conversations list display."""
        list_view = self.query_one("#conversations-list", ListView)
        list_view.clear()

        if not self.conversations:
            list_view.append(ListItem(Label("No conversations yet")))
            return

        for conversation in self.conversations:
            conv_id = conversation.get("id", "")
            participants = conversation.get("participants", "")
            last_message = conversation.get("last_message_at", "")[:19]

            # Parse participants (JSON array)
            try:
                participants_list = json.loads(participants) if participants else []
                # Filter out self (local actor)
                other_participants = [p for p in participants_list if "localhost" not in p and "dais.social" not in p]

                # Extract usernames
                participant_names = []
                for p in other_participants:
                    if "/" in p:
                        parts = p.split("/")
                        domain = parts[2] if len(parts) > 2 else ""
                        username = parts[-1] if len(parts) > 0 else ""
                        participant_names.append(f"{username}@{domain}")
                    else:
                        participant_names.append(p)

                display_name = ", ".join(participant_names) if participant_names else "Unknown"
            except:
                display_name = "Unknown"

            conv_text = f"💬 {display_name}\n[dim]{last_message}[/dim]"
            list_view.append(ConversationItem(conv_id, conversation, Label(conv_text)))

    def _load_conversation(self, conversation_id: str, conversation_data: dict) -> None:
        """Load and display messages for a conversation."""
        self.current_conversation = conversation_id

        # Update conversation title
        participants = conversation_data.get("participants", "")
        try:
            participants_list = json.loads(participants) if participants else []
            other_participants = [p for p in participants_list if "localhost" not in p and "dais.social" not in p]

            participant_names = []
            for p in other_participants:
                if "/" in p:
                    parts = p.split("/")
                    domain = parts[2] if len(parts) > 2 else ""
                    username = parts[-1] if len(parts) > 0 else ""
                    participant_names.append(f"{username}@{domain}")
                else:
                    participant_names.append(p)

            title = ", ".join(participant_names) if participant_names else "Unknown"
        except:
            title = "Unknown"

        self.query_one("#conversation-title", Static).update(f"Conversation with: {title}")

        # Load messages
        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        try:
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 f"""SELECT id, sender_id, content, published_at
                     FROM direct_messages
                     WHERE conversation_id = '{conversation_id}'
                     ORDER BY published_at ASC;"""],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )

            if result.returncode == 0:
                data = json.loads(result.stdout)
                if data and len(data) > 0 and "results" in data[0]:
                    self.messages = data[0]["results"]

        except Exception:
            self.messages = []

        self._update_messages_display()

    def _update_messages_display(self) -> None:
        """Update the messages display."""
        messages_content = self.query_one("#messages-content", Static)

        if not self.messages:
            messages_content.update("No messages yet. Start the conversation!")
            return

        # Build messages HTML
        html_parts = []
        for message in self.messages:
            sender = message.get("sender_id", "")
            content = message.get("content", "")
            timestamp = message.get("published_at", "")[:19]

            # Determine if message is from self or other
            is_sent = "localhost" in sender or "dais.social" in sender

            # Extract sender name
            if "/" in sender:
                parts = sender.split("/")
                sender_name = parts[-1] if len(parts) > 0 else "Unknown"
            else:
                sender_name = sender

            if is_sent:
                html_parts.append(f"[reverse]You:[/reverse] {content}\n[dim]{timestamp}[/dim]\n")
            else:
                html_parts.append(f"[bold]{sender_name}:[/bold] {content}\n[dim]{timestamp}[/dim]\n")

        messages_content.update("\n".join(html_parts))

    def _send_message(self) -> None:
        """Send a direct message."""
        if not self.current_conversation:
            self.notify("Please select a conversation first", severity="warning")
            return

        text_area = self.query_one("#message-input", TextArea)
        content = text_area.text.strip()

        if not content:
            self.notify("Message cannot be empty", severity="warning")
            return

        # Get recipient from current conversation
        conv_data = next((c for c in self.conversations if c.get("id") == self.current_conversation), None)
        if not conv_data:
            self.notify("Conversation not found", severity="error")
            return

        participants = conv_data.get("participants", "")
        try:
            participants_list = json.loads(participants) if participants else []
            recipients = [p for p in participants_list if "localhost" not in p and "dais.social" not in p]

            if not recipients:
                self.notify("No recipients found", severity="error")
                return

            # Use first recipient (for now, CLI only supports single recipient)
            recipient = recipients[0]
        except:
            self.notify("Invalid conversation data", severity="error")
            return

        project_root = Path(__file__).parent.parent.parent.parent.parent
        try:
            result = subprocess.run(
                ["dais", "dm", "send", recipient, content],
                capture_output=True,
                text=True,
                cwd=str(project_root)
            )

            if result.returncode == 0:
                self.notify("✓ Message sent", severity="information")
                text_area.text = ""
                self._load_conversation(self.current_conversation, conv_data)
            else:
                error_msg = result.stderr or result.stdout or "Unknown error"
                self.notify(f"Failed to send: {error_msg[:50]}", severity="error")

        except Exception as e:
            self.notify(f"Error: {str(e)}", severity="error")

    def _new_conversation(self) -> None:
        """Start a new DM conversation (placeholder)."""
        self.notify("New DM feature coming soon! Use CLI: dais dm send @user@domain 'message'", severity="information")

    def action_new_conversation(self) -> None:
        """Start a new conversation (alias)."""
        self._new_conversation()

    def action_switch_to_bluesky(self) -> None:
        """Switch to Bluesky chats screen."""
        from dais_cli.tui.screens.bluesky_chat import BlueskyChatsScreen
        self.app.pop_screen()  # Remove this screen
        self.app.push_screen(BlueskyChatsScreen())  # Show Bluesky chats
