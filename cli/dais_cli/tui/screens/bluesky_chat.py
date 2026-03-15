"""Bluesky Chat screen (chat.bsky.convo.* protocol)."""

import subprocess
import json
from pathlib import Path
from textual.app import ComposeResult
from textual.screen import Screen
from textual.containers import Container, Horizontal, Vertical, VerticalScroll
from textual.widgets import Static, ListView, ListItem, Label, Button, TextArea
from textual.binding import Binding


class BlueskyConversationItem(ListItem):
    """Custom list item for Bluesky conversations."""

    def __init__(self, conversation_id: str, conversation_data: dict, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.conversation_id = conversation_id
        self.conversation_data = conversation_data


class BlueskyChatsScreen(Screen):
    """Screen for managing Bluesky chats (separate from ActivityPub DMs)."""

    CSS = """
    BlueskyChatsScreen {
        align: center top;
        padding: 2;
    }

    #bluesky-chat-container {
        width: 100%;
        height: 100%;
        border: solid $primary;
        background: $surface;
        padding: 1;
    }

    #bluesky-chat-title {
        text-align: center;
        padding-bottom: 1;
        color: $primary;
    }

    #protocol-notice {
        text-align: center;
        padding: 1;
        background: $accent-darken-2;
        color: $warning;
    }

    #chat-layout {
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

    .unread-badge {
        color: $error;
    }
    """

    BINDINGS = [
        Binding("r", "refresh", "Refresh", show=True),
        Binding("a", "switch_to_activitypub", "ActivityPub DMs", show=True),
        Binding("escape", "app.pop_screen", "Back", show=True),
    ]

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.conversations = []
        self.current_conversation = None
        self.messages = []

    def compose(self) -> ComposeResult:
        """Compose the Bluesky chat UI."""
        with Container(id="bluesky-chat-container"):
            yield Static("🦋 **Bluesky Chats**", id="bluesky-chat-title")

            yield Static(
                "Protocol: chat.bsky.convo.* (separate from ActivityPub DMs)",
                id="protocol-notice"
            )

            with Horizontal(id="chat-layout"):
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
                yield Button("✉️ New Chat", id="new-chat-btn", variant="success")
                yield Button("📬 ActivityPub DMs", id="switch-protocol-btn", variant="default")
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
        elif event.button.id == "new-chat-btn":
            self._new_conversation()
        elif event.button.id == "switch-protocol-btn":
            self.action_switch_to_activitypub()

    def on_list_view_selected(self, event: ListView.Selected) -> None:
        """Handle conversation selection."""
        if event.list_view.id == "conversations-list" and isinstance(event.item, BlueskyConversationItem):
            self._load_conversation(event.item.conversation_id, event.item.conversation_data)

    def action_refresh(self) -> None:
        """Refresh conversations list from database."""
        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        try:
            # Query Bluesky conversations with most recent message time
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 """SELECT c.id, c.participants, c.last_message_at, c.last_message_text, c.unread_count
                    FROM bluesky_conversations c
                    ORDER BY c.last_message_at DESC;"""],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )

            if result.returncode == 0:
                # Parse JSON from wrangler output
                start = result.stdout.find('[')
                end = result.stdout.rfind(']') + 1
                if start >= 0 and end > 0:
                    data = json.loads(result.stdout[start:end])
                    if data and len(data) > 0 and "results" in data[0]:
                        self.conversations = data[0]["results"]

        except Exception:
            self.conversations = []

        self._update_conversations_display()
        self.notify("Bluesky chats refreshed", severity="information")

    def _update_conversations_display(self) -> None:
        """Update the conversations list display."""
        list_view = self.query_one("#conversations-list", ListView)
        list_view.clear()

        if not self.conversations:
            list_view.append(ListItem(Label("No Bluesky chats yet\n[dim]Chats use chat.bsky.convo.* protocol[/dim]")))
            return

        for conversation in self.conversations:
            conv_id = conversation.get("id", "")
            participants = conversation.get("participants", "")
            last_message = conversation.get("last_message_at", "")[:19]
            preview = conversation.get("last_message_text", "")[:40]
            unread_count = conversation.get("unread_count", 0)

            # Parse participants (JSON array of DIDs)
            try:
                participants_list = json.loads(participants) if participants else []
                # Filter out self
                other_participants = [p for p in participants_list if "did:web:social.dais.social" not in p]

                # Extract handles from DIDs (simplified)
                participant_names = []
                for p in other_participants:
                    # DIDs look like: did:plc:abc123 or did:web:alice.bsky.social
                    if "did:web:" in p:
                        handle = p.replace("did:web:", "")
                        participant_names.append(handle)
                    else:
                        # For did:plc:, would need DID resolution
                        participant_names.append(p[-10:])  # Last 10 chars

                display_name = ", ".join(participant_names) if participant_names else "Unknown"
            except:
                display_name = "Unknown"

            # Unread badge
            unread_badge = f"[bold red]({unread_count})[/bold red] " if unread_count > 0 else ""

            conv_text = f"🦋 {unread_badge}{display_name}\n[dim]{preview}... • {last_message}[/dim]"
            list_view.append(BlueskyConversationItem(conv_id, conversation, Label(conv_text)))

    def _load_conversation(self, conversation_id: str, conversation_data: dict) -> None:
        """Load and display messages for a Bluesky conversation."""
        self.current_conversation = conversation_id

        # Update conversation title
        participants = conversation_data.get("participants", "")
        try:
            participants_list = json.loads(participants) if participants else []
            other_participants = [p for p in participants_list if "did:web:social.dais.social" not in p]

            participant_names = []
            for p in other_participants:
                if "did:web:" in p:
                    handle = p.replace("did:web:", "")
                    participant_names.append(handle)
                else:
                    participant_names.append(p[-10:])

            title = ", ".join(participant_names) if participant_names else "Unknown"
        except:
            title = "Unknown"

        self.query_one("#conversation-title", Static).update(f"Chat with: {title} [dim](Bluesky)[/dim]")

        # Load messages
        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        try:
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 f"""SELECT id, sender_did, sender_handle, text, sent_at, read
                     FROM bluesky_messages
                     WHERE conversation_id = '{conversation_id}'
                     ORDER BY sent_at ASC;"""],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )

            if result.returncode == 0:
                start = result.stdout.find('[')
                end = result.stdout.rfind(']') + 1
                if start >= 0 and end > 0:
                    data = json.loads(result.stdout[start:end])
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

        # Build messages display
        html_parts = []
        for message in self.messages:
            sender_did = message.get("sender_did", "")
            sender_handle = message.get("sender_handle", sender_did)
            text = message.get("text", "")
            timestamp = message.get("sent_at", "")[:19]
            is_read = message.get("read", 0) == 1

            # Determine if message is from self
            is_sent = "did:web:social.dais.social" in sender_did

            if is_sent:
                html_parts.append(f"[reverse]You:[/reverse] {text}\n[dim]{timestamp}[/dim]\n")
            else:
                read_marker = "" if is_read else "[bold red]●[/bold red] "
                html_parts.append(f"{read_marker}[bold]{sender_handle}:[/bold] {text}\n[dim]{timestamp}[/dim]\n")

        messages_content.update("\n".join(html_parts))

    def _send_message(self) -> None:
        """Send a message via Bluesky chat API."""
        if not self.current_conversation:
            self.notify("Please select a conversation first", severity="warning")
            return

        text_area = self.query_one("#message-input", TextArea)
        content = text_area.text.strip()

        if not content:
            self.notify("Message cannot be empty", severity="warning")
            return

        # Send message via Bluesky chat API
        try:
            import httpx
            from datetime import datetime
            from pathlib import Path

            # Get PDS credentials
            dais_dir = Path.home() / ".dais"
            password_path = dais_dir / "pds-password.txt"

            if not password_path.exists():
                self.notify("PDS password not found. Check ~/.dais/pds-password.txt", severity="error")
                return

            with open(password_path) as f:
                password = f.read().strip()

            # Determine PDS URL based on config
            config = Config()
            config.load()
            domain = config.get("server.pds_domain", "pds.dais.social")
            pds_url = f"https://{domain}"

            # Authenticate with PDS
            auth_response = httpx.post(
                f"{pds_url}/xrpc/com.atproto.server.createSession",
                json={
                    "identifier": "social.dais.social",
                    "password": password
                },
                timeout=30.0
            )

            if auth_response.status_code != 200:
                self.notify(f"PDS authentication failed: {auth_response.status_code}", severity="error")
                return

            session = auth_response.json()
            access_token = session.get("accessJwt")

            # Send message via chat.bsky.convo.sendMessage
            message_response = httpx.post(
                f"{pds_url}/xrpc/chat.bsky.convo.sendMessage",
                json={
                    "convoId": self.current_conversation,
                    "message": {
                        "$type": "chat.bsky.convo.defs#messageInput",
                        "text": content
                    }
                },
                headers={"Authorization": f"Bearer {access_token}"},
                timeout=30.0
            )

            if message_response.status_code == 200:
                self.notify("✓ Message sent", severity="information")
                text_area.text = ""  # Clear input

                # Refresh messages to show the sent message
                self._load_messages()
            else:
                error_text = message_response.text[:100]
                self.notify(f"Failed to send message: {message_response.status_code}", severity="error")
                self.notify(f"Error: {error_text}", severity="error")

        except Exception as e:
            self.notify(f"Error sending message: {str(e)[:100]}", severity="error")

    def _new_conversation(self) -> None:
        """Start a new Bluesky chat."""
        # Show modal for DID/handle input
        from dais_cli.tui.widgets.input_modal import InputModal

        def handle_modal_result(did_or_handle: str | None) -> None:
            if not did_or_handle:
                return

            self._create_conversation_with_user(did_or_handle)

        self.app.push_screen(
            InputModal(
                title="New Bluesky Chat",
                description="Enter DID or handle (e.g., alice.bsky.social)",
                placeholder="alice.bsky.social or did:plc:...",
            ),
            handle_modal_result
        )

    def _create_conversation_with_user(self, did_or_handle: str) -> None:
        """Create a new conversation with a user.

        Args:
            did_or_handle: User's DID or handle
        """
        try:
            import httpx
            from pathlib import Path

            # Get PDS credentials
            dais_dir = Path.home() / ".dais"
            password_path = dais_dir / "pds-password.txt"

            if not password_path.exists():
                self.notify("PDS password not found", severity="error")
                return

            with open(password_path) as f:
                password = f.read().strip()

            # Determine PDS URL
            config = Config()
            config.load()
            domain = config.get("server.pds_domain", "pds.dais.social")
            pds_url = f"https://{domain}"

            # Authenticate
            auth_response = httpx.post(
                f"{pds_url}/xrpc/com.atproto.server.createSession",
                json={
                    "identifier": "social.dais.social",
                    "password": password
                },
                timeout=30.0
            )

            if auth_response.status_code != 200:
                self.notify("PDS authentication failed", severity="error")
                return

            session = auth_response.json()
            access_token = session.get("accessJwt")

            # Resolve handle to DID if needed
            recipient_did = did_or_handle
            if not did_or_handle.startswith("did:"):
                # Resolve handle to DID
                resolve_response = httpx.get(
                    f"{pds_url}/xrpc/com.atproto.identity.resolveHandle",
                    params={"handle": did_or_handle},
                    headers={"Authorization": f"Bearer {access_token}"},
                    timeout=30.0
                )
                if resolve_response.status_code == 200:
                    recipient_did = resolve_response.json().get("did")
                else:
                    self.notify(f"Could not resolve handle: {did_or_handle}", severity="error")
                    return

            # Create conversation
            convo_response = httpx.post(
                f"{pds_url}/xrpc/chat.bsky.convo.createConvo",
                json={"members": [recipient_did]},
                headers={"Authorization": f"Bearer {access_token}"},
                timeout=30.0
            )

            if convo_response.status_code == 200:
                self.notify("✓ Conversation created", severity="information")
                self.action_refresh()  # Reload conversations
            else:
                error_text = convo_response.text[:100]
                self.notify(f"Failed to create conversation: {error_text}", severity="error")

        except Exception as e:
            self.notify(f"Error: {str(e)[:100]}", severity="error")

    def action_switch_to_activitypub(self) -> None:
        """Switch to ActivityPub DMs screen."""
        from dais_cli.tui.screens.direct_messages import DirectMessagesScreen
        self.app.pop_screen()  # Remove this screen
        self.app.push_screen(DirectMessagesScreen())  # Show ActivityPub DMs
