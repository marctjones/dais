"""Post list widget for dashboard."""

import subprocess
import json
from pathlib import Path
from textual.app import ComposeResult
from textual.widgets import Static, ListView, ListItem, Label
from textual.containers import Vertical


class PostListWidget(Static):
    """Display recent posts."""

    DEFAULT_CSS = """
    PostListWidget {
        height: 100%;
        border: solid $primary;
        padding: 1;
    }
    """

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.posts = []

    def compose(self) -> ComposeResult:
        """Compose the post list."""
        with Vertical():
            yield Static("📝 **Recent Posts**", classes="list-header")
            yield ListView(id="post-list-view")

    def refresh_data(self) -> None:
        """Fetch recent posts from database."""
        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        try:
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 "SELECT id, content, published_at FROM posts ORDER BY published_at DESC LIMIT 5;"],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )

            if result.returncode == 0:
                data = json.loads(result.stdout)
                if data and len(data) > 0 and "results" in data[0]:
                    self.posts = data[0]["results"]

        except Exception:
            self.posts = []

        self._update_display()

    def _update_display(self) -> None:
        """Update the post list with current posts."""
        list_view = self.query_one("#post-list-view", ListView)
        list_view.clear()

        if not self.posts:
            list_view.append(ListItem(Label("No posts yet")))
            return

        for post in self.posts:
            # Truncate content to 60 chars
            content = post.get("content", "")
            if len(content) > 60:
                content = content[:57] + "..."

            # Extract post ID (last part after /)
            post_id = post.get("id", "").split("/")[-1] if post.get("id") else "unknown"
            published = post.get("published_at", "")[:19]  # Truncate timestamp

            post_text = f"{content}\n[dim]{published} • {post_id}[/dim]"
            list_view.append(ListItem(Label(post_text)))

    def on_mount(self) -> None:
        """Load posts when widget is mounted."""
        self.refresh_data()
