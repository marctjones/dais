"""Stats widget for dashboard."""

import subprocess
from textual.app import ComposeResult
from textual.widgets import Static
from textual.containers import Vertical
from pathlib import Path


class StatsWidget(Static):
    """Display server statistics."""

    DEFAULT_CSS = """
    StatsWidget {
        height: auto;
        border: solid $primary;
        padding: 1;
    }

    .stat-row {
        padding: 0 1;
    }
    """

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.stats = {
            "followers": 0,
            "posts": 0,
            "pending": 0,
        }

    def compose(self) -> ComposeResult:
        """Compose the stats display."""
        with Vertical():
            yield Static("📊 **Stats**", classes="stat-header")
            yield Static("", id="stats-content")

    def refresh_data(self) -> None:
        """Fetch latest statistics from database."""
        # Get project root
        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        try:
            # Get follower counts
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 "SELECT COUNT(*) as count FROM followers WHERE status='approved';"],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )
            if result.returncode == 0 and "count" in result.stdout:
                import json
                data = json.loads(result.stdout)
                if data and len(data) > 0 and "results" in data[0]:
                    self.stats["followers"] = data[0]["results"][0]["count"]

            # Get pending follower requests
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 "SELECT COUNT(*) as count FROM followers WHERE status='pending';"],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )
            if result.returncode == 0 and "count" in result.stdout:
                import json
                data = json.loads(result.stdout)
                if data and len(data) > 0 and "results" in data[0]:
                    self.stats["pending"] = data[0]["results"][0]["count"]

            # Get post count
            result = subprocess.run(
                ["wrangler", "d1", "execute", "DB", "--local", "--command",
                 "SELECT COUNT(*) as count FROM posts;"],
                capture_output=True,
                text=True,
                cwd=str(worker_dir)
            )
            if result.returncode == 0 and "count" in result.stdout:
                import json
                data = json.loads(result.stdout)
                if data and len(data) > 0 and "results" in data[0]:
                    self.stats["posts"] = data[0]["results"][0]["count"]

        except Exception as e:
            # Silently fail - show zeros
            pass

        # Update display
        self._update_display()

    def _update_display(self) -> None:
        """Update the stats display with current values."""
        content = self.query_one("#stats-content", Static)

        stats_text = f"""
👥 Followers: {self.stats['followers']}
📬 Pending: {self.stats['pending']}
📝 Posts: {self.stats['posts']}
        """.strip()

        content.update(stats_text)

    def on_mount(self) -> None:
        """Load stats when widget is mounted."""
        self.refresh_data()
