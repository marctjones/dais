"""Thread viewer screen for viewing posts with replies and interactions."""

import subprocess
import json
from pathlib import Path
from textual.app import ComposeResult
from textual.screen import Screen
from textual.containers import Container, Horizontal, Vertical, VerticalScroll
from textual.widgets import Static, Button, TextArea, Input
from textual.binding import Binding


class ThreadScreen(Screen):
    """Screen for viewing a post thread with replies and interactions."""

    CSS = """
    ThreadScreen {
        align: center top;
        padding: 2;
    }

    #thread-container {
        width: 100%;
        height: 100%;
        border: solid $primary;
        background: $surface;
        padding: 1;
    }

    #thread-title {
        text-align: center;
        padding-bottom: 1;
        color: $primary;
    }

    #thread-scroll {
        height: 1fr;
        border: solid $accent;
        padding: 1;
        margin-bottom: 1;
    }

    #compose-reply-section {
        height: auto;
        border: solid $accent;
        padding: 1;
    }

    #reply-input {
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

    .original-post {
        background: $primary-darken-2;
        padding: 1;
        margin-bottom: 2;
        border-radius: 1;
    }

    .reply-post {
        background: $accent-darken-2;
        padding: 1;
        margin: 1 0 1 2;
        border-radius: 1;
        border-left: thick $accent;
    }

    .interaction-counts {
        color: $text-muted;
        padding: 1 0;
    }

    .protocol-badge {
        color: $accent;
    }
    """

    BINDINGS = [
        Binding("r", "refresh", "Refresh", show=True),
        Binding("l", "like_post", "Like", show=True),
        Binding("b", "boost_post", "Boost", show=True),
        Binding("escape", "app.pop_screen", "Back", show=True),
    ]

    def __init__(self, post_id: str = None, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.post_id = post_id
        self.post_data = None
        self.replies = []
        self.interactions = {}

    def compose(self) -> ComposeResult:
        """Compose the thread viewer UI."""
        with Container(id="thread-container"):
            yield Static("🧵 **Thread Viewer**", id="thread-title")

            with VerticalScroll(id="thread-scroll"):
                yield Static("Loading thread...", id="thread-content")

            with Container(id="compose-reply-section"):
                yield Static("Reply to this thread:")
                yield TextArea("", id="reply-input", language="markdown")
                with Horizontal():
                    yield Button("↩️ Reply", id="reply-btn", variant="primary")
                    yield Button("🗑️ Clear", id="clear-reply-btn", variant="default")

            with Horizontal(classes="actions"):
                yield Button("❤️ Like", id="like-btn", variant="default")
                yield Button("🔁 Boost", id="boost-btn", variant="default")
                yield Button("🔄 Refresh", id="refresh-btn", variant="default")
                yield Button("← Back", id="back-btn", variant="default")

    def on_mount(self) -> None:
        """Load thread when screen is mounted."""
        if self.post_id:
            self.action_refresh()
        else:
            self.query_one("#thread-content", Static).update("No post selected")

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle button presses."""
        if event.button.id == "refresh-btn":
            self.action_refresh()
        elif event.button.id == "back-btn":
            self.app.pop_screen()
        elif event.button.id == "reply-btn":
            self._post_reply()
        elif event.button.id == "clear-reply-btn":
            self.query_one("#reply-input", TextArea).text = ""
        elif event.button.id == "like-btn":
            self._like_post()
        elif event.button.id == "boost-btn":
            self._boost_post()

    def action_refresh(self) -> None:
        """Refresh thread data from database."""
        if not self.post_id:
            return

        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        try:
            # Load post data
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 f"""SELECT p.id, p.content, p.content_html, p.published_at, p.visibility,
                            p.in_reply_to, p.protocol, p.atproto_uri
                     FROM posts p
                     WHERE p.id LIKE '%{self.post_id}%';"""],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )

            if result.returncode == 0:
                data = json.loads(result.stdout)
                if data and len(data) > 0 and "results" in data[0] and data[0]["results"]:
                    self.post_data = data[0]["results"][0]

            # Load replies
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 f"""SELECT r.id, r.actor_username, r.actor_display_name, r.content,
                            r.published_at, r.moderation_status, r.hidden
                     FROM replies r
                     WHERE r.post_id LIKE '%{self.post_id}%'
                     ORDER BY r.published_at ASC;"""],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )

            if result.returncode == 0:
                data = json.loads(result.stdout)
                if data and len(data) > 0 and "results" in data[0]:
                    # Filter out hidden/rejected replies
                    all_replies = data[0]["results"]
                    self.replies = [r for r in all_replies
                                   if r.get("hidden") != 1 and r.get("moderation_status") != "rejected"]

            # Load interaction counts
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 f"""SELECT type, COUNT(*) as count
                     FROM interactions
                     WHERE post_id LIKE '%{self.post_id}%'
                     GROUP BY type;"""],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )

            if result.returncode == 0:
                data = json.loads(result.stdout)
                if data and len(data) > 0 and "results" in data[0]:
                    self.interactions = {row["type"]: row["count"] for row in data[0]["results"]}

        except Exception as e:
            self.notify(f"Error loading thread: {str(e)}", severity="error")

        self._update_display()

    def _update_display(self) -> None:
        """Update the thread display."""
        thread_content = self.query_one("#thread-content", Static)

        if not self.post_data:
            thread_content.update("Post not found")
            return

        # Build thread display
        parts = []

        # Original post
        content = self.post_data.get("content", "")
        published = self.post_data.get("published_at", "")[:19]
        protocol = self.post_data.get("protocol", "activitypub")
        atproto_uri = self.post_data.get("atproto_uri", "")

        # Protocol badge
        protocol_badge = ""
        if protocol == "both":
            protocol_badge = " [dim][🦋 AP + AT][/dim]"
        elif protocol == "atproto":
            protocol_badge = " [dim][🦋 Bluesky][/dim]"
        elif protocol == "activitypub":
            protocol_badge = " [dim][🐘 ActivityPub][/dim]"

        parts.append(f"[bold]Original Post[/bold]{protocol_badge}")
        parts.append(f"{content}")
        parts.append(f"[dim]{published}[/dim]")

        # Interaction counts
        likes = self.interactions.get("like", 0)
        boosts = self.interactions.get("boost", 0)
        parts.append(f"[dim]❤️ {likes} likes • 🔁 {boosts} boosts • 💬 {len(self.replies)} replies[/dim]")

        # AT Protocol URI if available
        if atproto_uri:
            parts.append(f"[dim]AT URI: {atproto_uri}[/dim]")

        parts.append("\n---\n")

        # Replies
        if self.replies:
            parts.append("[bold]Replies:[/bold]\n")
            for reply in self.replies:
                username = reply.get("actor_username", "unknown")
                display_name = reply.get("actor_display_name", "")
                reply_content = reply.get("content", "")
                reply_time = reply.get("published_at", "")[:19]

                author = display_name if display_name else username
                parts.append(f"  ↳ [bold]{author}[/bold] (@{username})")
                parts.append(f"    {reply_content}")
                parts.append(f"    [dim]{reply_time}[/dim]\n")
        else:
            parts.append("[dim]No replies yet. Be the first to reply![/dim]")

        thread_content.update("\n".join(parts))

    def _post_reply(self) -> None:
        """Post a reply to this thread."""
        if not self.post_id:
            self.notify("No post to reply to", severity="error")
            return

        text_area = self.query_one("#reply-input", TextArea)
        content = text_area.text.strip()

        if not content:
            self.notify("Reply cannot be empty", severity="warning")
            return

        project_root = Path(__file__).parent.parent.parent.parent.parent
        try:
            # Extract just the post ID (last part of URL)
            post_id_short = self.post_id.split("/")[-1] if "/" in self.post_id else self.post_id

            result = subprocess.run(
                ["dais", "interact", "reply", post_id_short, content],
                capture_output=True,
                text=True,
                cwd=str(project_root)
            )

            if result.returncode == 0:
                self.notify("✓ Reply posted", severity="information")
                text_area.text = ""
                self.action_refresh()
            else:
                error_msg = result.stderr or result.stdout or "Unknown error"
                self.notify(f"Failed to reply: {error_msg[:50]}", severity="error")

        except Exception as e:
            self.notify(f"Error: {str(e)}", severity="error")

    def _like_post(self) -> None:
        """Like this post."""
        if not self.post_id:
            return

        project_root = Path(__file__).parent.parent.parent.parent.parent
        try:
            post_id_short = self.post_id.split("/")[-1] if "/" in self.post_id else self.post_id

            result = subprocess.run(
                ["dais", "interact", "like", post_id_short],
                capture_output=True,
                text=True,
                cwd=str(project_root)
            )

            if result.returncode == 0:
                self.notify("❤️ Post liked", severity="information")
                self.action_refresh()
            else:
                error_msg = result.stderr or result.stdout or "Unknown error"
                self.notify(f"Failed to like: {error_msg[:50]}", severity="error")

        except Exception as e:
            self.notify(f"Error: {str(e)}", severity="error")

    def _boost_post(self) -> None:
        """Boost (reblog) this post."""
        if not self.post_id:
            return

        project_root = Path(__file__).parent.parent.parent.parent.parent
        try:
            post_id_short = self.post_id.split("/")[-1] if "/" in self.post_id else self.post_id

            result = subprocess.run(
                ["dais", "interact", "boost", post_id_short],
                capture_output=True,
                text=True,
                cwd=str(project_root)
            )

            if result.returncode == 0:
                self.notify("🔁 Post boosted", severity="information")
                self.action_refresh()
            else:
                error_msg = result.stderr or result.stdout or "Unknown error"
                self.notify(f"Failed to boost: {error_msg[:50]}", severity="error")

        except Exception as e:
            self.notify(f"Error: {str(e)}", severity="error")

    def action_like_post(self) -> None:
        """Like the post (keyboard shortcut)."""
        self._like_post()

    def action_boost_post(self) -> None:
        """Boost the post (keyboard shortcut)."""
        self._boost_post()
