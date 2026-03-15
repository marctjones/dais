"""Analytics Dashboard Screen - Detailed metrics and statistics."""

import json
import subprocess
from datetime import datetime, timedelta
from pathlib import Path
from textual.app import ComposeResult
from textual.containers import Container, Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import Static, DataTable, Label
from textual.binding import Binding
from rich.text import Text


class AnalyticsScreen(Screen):
    """Analytics Dashboard with detailed metrics."""

    CSS = """
    AnalyticsScreen {
        background: $surface;
    }

    #title {
        color: $accent;
        text-style: bold;
        padding: 1;
        text-align: center;
    }

    #stats-container {
        height: auto;
        margin: 1 2;
    }

    .stat-box {
        border: solid $primary;
        background: $surface-darken-1;
        padding: 1 2;
        margin: 0 1;
        height: auto;
        width: 1fr;
    }

    .stat-value {
        color: $accent;
        text-style: bold;
        text-align: center;
    }

    .stat-label {
        color: $text-muted;
        text-align: center;
    }

    #charts-container {
        margin: 1 2;
        height: auto;
    }

    #growth-table {
        height: 15;
        margin: 1 2;
    }
    """

    BINDINGS = [
        Binding("r", "refresh", "Refresh", show=True),
        Binding("escape", "app.pop_screen", "Back", show=True),
    ]

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.stats = {}
        self.growth_data = []

    def compose(self) -> ComposeResult:
        """Compose the analytics UI."""
        yield Static("📊 Analytics Dashboard", id="title")

        with Container(id="stats-container"):
            with Horizontal():
                # Total stats boxes
                with Vertical(classes="stat-box"):
                    yield Static("0", classes="stat-value", id="total-posts")
                    yield Static("Total Posts", classes="stat-label")

                with Vertical(classes="stat-box"):
                    yield Static("0", classes="stat-value", id="total-followers")
                    yield Static("Followers", classes="stat-label")

                with Vertical(classes="stat-box"):
                    yield Static("0", classes="stat-value", id="total-replies")
                    yield Static("Replies", classes="stat-label")

                with Vertical(classes="stat-box"):
                    yield Static("0", classes="stat-value", id="total-notifications")
                    yield Static("Notifications", classes="stat-label")

        with Container(id="stats-container"):
            with Horizontal():
                # Engagement stats
                with Vertical(classes="stat-box"):
                    yield Static("0%", classes="stat-value", id="reply-rate")
                    yield Static("Reply Rate", classes="stat-label")

                with Vertical(classes="stat-box"):
                    yield Static("0", classes="stat-value", id="avg-replies")
                    yield Static("Avg Replies/Post", classes="stat-label")

                with Vertical(classes="stat-box"):
                    yield Static("0%", classes="stat-value", id="approval-rate")
                    yield Static("Approval Rate", classes="stat-label")

                with Vertical(classes="stat-box"):
                    yield Static("0%", classes="stat-value", id="dual-protocol-pct")
                    yield Static("Dual-Protocol", classes="stat-label")

        # Growth table
        table = DataTable(id="growth-table")
        table.add_column("Period", width=20)
        table.add_column("Posts", width=10)
        table.add_column("Followers", width=12)
        table.add_column("Replies", width=10)
        table.add_column("Growth", width=15)
        yield table

    def on_mount(self) -> None:
        """Load analytics when screen mounts."""
        self.action_refresh()

    def action_refresh(self) -> None:
        """Refresh analytics data."""
        self.notify("Refreshing analytics...", severity="information")
        self._load_stats()
        self._load_growth_data()
        self._update_display()

    def _load_stats(self) -> None:
        """Load statistics from database."""
        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        # Get comprehensive stats
        query = """
        SELECT
            (SELECT COUNT(*) FROM posts) as total_posts,
            (SELECT COUNT(*) FROM posts WHERE protocol = 'both') as dual_protocol_posts,
            (SELECT COUNT(*) FROM followers WHERE status='accepted') as total_followers,
            (SELECT COUNT(*) FROM followers WHERE status='pending') as pending_followers,
            (SELECT COUNT(*) FROM replies) as total_replies,
            (SELECT COUNT(*) FROM replies WHERE moderation_status='approved') as approved_replies,
            (SELECT COUNT(*) FROM notifications) as total_notifications,
            (SELECT COUNT(*) FROM notifications WHERE read=0) as unread_notifications
        """

        cmd = ["wrangler", "d1", "execute", "DB", "--local", "--command", query]

        try:
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                check=True,
                cwd=str(worker_dir)
            )

            # Parse JSON
            start = result.stdout.find('[')
            end = result.stdout.rfind(']') + 1
            if start >= 0 and end > 0:
                data = json.loads(result.stdout[start:end])
                if data and len(data) > 0 and "results" in data[0] and data[0]["results"]:
                    self.stats = data[0]["results"][0]
        except Exception as e:
            self.notify(f"Failed to load stats: {e}", severity="error")
            self.stats = {}

    def _load_growth_data(self) -> None:
        """Load growth data over time."""
        project_root = Path(__file__).parent.parent.parent.parent.parent
        worker_dir = project_root / "workers" / "actor"

        # Get daily growth for last 7 days
        self.growth_data = []

        for days_ago in range(6, -1, -1):
            date = (datetime.now() - timedelta(days=days_ago)).strftime('%Y-%m-%d')

            query = f"""
            SELECT
                (SELECT COUNT(*) FROM posts WHERE DATE(published_at) = '{date}') as posts,
                (SELECT COUNT(*) FROM followers WHERE DATE(created_at) = '{date}') as followers,
                (SELECT COUNT(*) FROM replies WHERE DATE(published_at) = '{date}') as replies
            """

            cmd = ["wrangler", "d1", "execute", "DB", "--local", "--command", query]

            try:
                result = subprocess.run(
                    cmd,
                    capture_output=True,
                    text=True,
                    check=True,
                    cwd=str(worker_dir)
                )

                start = result.stdout.find('[')
                end = result.stdout.rfind(']') + 1
                if start >= 0 and end > 0:
                    data = json.loads(result.stdout[start:end])
                    if data and len(data) > 0 and "results" in data[0] and data[0]["results"]:
                        day_stats = data[0]["results"][0]
                        self.growth_data.append({
                            "date": date,
                            **day_stats
                        })
            except Exception:
                pass

    def _update_display(self) -> None:
        """Update UI with loaded data."""
        if not self.stats:
            return

        # Update stat boxes
        total_posts = self.stats.get('total_posts', 0)
        total_followers = self.stats.get('total_followers', 0)
        total_replies = self.stats.get('total_replies', 0)
        total_notifications = self.stats.get('total_notifications', 0)
        dual_protocol_posts = self.stats.get('dual_protocol_posts', 0)
        approved_replies = self.stats.get('approved_replies', 0)
        pending_followers = self.stats.get('pending_followers', 0)

        # Update totals
        self.query_one("#total-posts", Static).update(str(total_posts))
        self.query_one("#total-followers", Static).update(str(total_followers))
        self.query_one("#total-replies", Static).update(str(total_replies))
        self.query_one("#total-notifications", Static).update(str(total_notifications))

        # Calculate metrics
        reply_rate = (total_replies / total_posts * 100) if total_posts > 0 else 0
        avg_replies = (total_replies / total_posts) if total_posts > 0 else 0
        approval_rate = (approved_replies / total_replies * 100) if total_replies > 0 else 0
        dual_protocol_pct = (dual_protocol_posts / total_posts * 100) if total_posts > 0 else 0

        self.query_one("#reply-rate", Static).update(f"{reply_rate:.1f}%")
        self.query_one("#avg-replies", Static).update(f"{avg_replies:.1f}")
        self.query_one("#approval-rate", Static).update(f"{approval_rate:.1f}%")
        self.query_one("#dual-protocol-pct", Static).update(f"{dual_protocol_pct:.1f}%")

        # Update growth table
        table = self.query_one("#growth-table", DataTable)
        table.clear()

        for day_data in self.growth_data:
            date = day_data['date']
            posts = day_data.get('posts', 0)
            followers = day_data.get('followers', 0)
            replies = day_data.get('replies', 0)

            # Calculate growth indicator
            total_activity = posts + followers + replies
            if total_activity > 5:
                growth = "📈 High"
            elif total_activity > 0:
                growth = "→ Normal"
            else:
                growth = "📉 Low"

            # Format date (show day of week)
            date_obj = datetime.strptime(date, '%Y-%m-%d')
            date_str = date_obj.strftime('%a, %b %d')

            table.add_row(
                date_str,
                str(posts),
                f"+{followers}",
                str(replies),
                growth
            )

        self.notify("Analytics refreshed", severity="information")
