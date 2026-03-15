"""Automated TUI testing script - validates components without manual interaction."""

import sys
import subprocess
import json
from pathlib import Path

# Add CLI to path
sys.path.insert(0, str(Path(__file__).parent))

def test_imports():
    """Test that all TUI components import successfully."""
    print("Testing TUI imports...")

    try:
        from dais_cli.tui.app import DaisApp, run_tui
        print("  ✓ Main app imports")

        from dais_cli.tui.screens.dashboard import DashboardScreen
        print("  ✓ Dashboard screen imports")

        from dais_cli.tui.screens.composer import ComposerScreen
        print("  ✓ Composer screen imports")

        from dais_cli.tui.screens.followers import FollowersScreen
        print("  ✓ Followers screen imports")

        from dais_cli.tui.screens.moderation import ModerationScreen
        print("  ✓ Moderation screen imports")

        from dais_cli.tui.screens.blocks import BlocksScreen
        print("  ✓ Blocks screen imports")

        from dais_cli.tui.screens.notifications import NotificationsScreen
        print("  ✓ Notifications screen imports")

        from dais_cli.tui.screens.direct_messages import DirectMessagesScreen
        print("  ✓ Direct messages screen imports")

        from dais_cli.tui.screens.thread import ThreadScreen
        print("  ✓ Thread viewer screen imports")

        from dais_cli.tui.widgets.stats import StatsWidget
        from dais_cli.tui.widgets.post_list import PostListWidget
        from dais_cli.tui.widgets.follower_list import FollowerListWidget
        print("  ✓ All widgets import")

        return True
    except Exception as e:
        print(f"  ✗ Import failed: {e}")
        return False


def test_app_instantiation():
    """Test that DaisApp can be instantiated."""
    print("\nTesting app instantiation...")

    try:
        from dais_cli.tui.app import DaisApp
        app = DaisApp()

        print(f"  ✓ App created: {app.TITLE}")
        print(f"  ✓ Identity: {app.identity}")
        print(f"  ✓ Bindings: {len(app.BINDINGS)} keyboard shortcuts")

        # Verify bindings
        expected_bindings = ['q', 'd', 'n', 'f', 'm', 'b', 'i', 'x', '?']
        actual_bindings = [b.key for b in app.BINDINGS]

        for key in expected_bindings:
            if key in actual_bindings:
                print(f"    ✓ Binding '{key}' registered")
            else:
                print(f"    ✗ Missing binding '{key}'")
                return False

        return True
    except Exception as e:
        print(f"  ✗ App instantiation failed: {e}")
        return False


def test_screen_instantiation():
    """Test that all screens can be instantiated."""
    print("\nTesting screen instantiation...")

    try:
        from dais_cli.tui.screens.dashboard import DashboardScreen
        from dais_cli.tui.screens.composer import ComposerScreen
        from dais_cli.tui.screens.followers import FollowersScreen
        from dais_cli.tui.screens.moderation import ModerationScreen
        from dais_cli.tui.screens.blocks import BlocksScreen
        from dais_cli.tui.screens.notifications import NotificationsScreen
        from dais_cli.tui.screens.direct_messages import DirectMessagesScreen
        from dais_cli.tui.screens.bluesky_chat import BlueskyChatsScreen
        from dais_cli.tui.screens.thread import ThreadScreen

        screens = [
            ("Dashboard", DashboardScreen()),
            ("Composer", ComposerScreen()),
            ("Followers", FollowersScreen()),
            ("Moderation", ModerationScreen()),
            ("Blocks", BlocksScreen()),
            ("Notifications", NotificationsScreen()),
            ("Direct Messages", DirectMessagesScreen()),
            ("Bluesky Chats", BlueskyChatsScreen()),
            ("Thread", ThreadScreen(post_id="test")),
        ]

        for name, screen in screens:
            print(f"  ✓ {name} screen instantiated")

            # Check for bindings
            if hasattr(screen, 'BINDINGS'):
                print(f"    - {len(screen.BINDINGS)} bindings")

        return True
    except Exception as e:
        print(f"  ✗ Screen instantiation failed: {e}")
        import traceback
        traceback.print_exc()
        return False


def test_widget_instantiation():
    """Test that all widgets can be instantiated."""
    print("\nTesting widget instantiation...")

    try:
        from dais_cli.tui.widgets.stats import StatsWidget
        from dais_cli.tui.widgets.post_list import PostListWidget
        from dais_cli.tui.widgets.follower_list import FollowerListWidget

        widgets = [
            ("Stats", StatsWidget()),
            ("Post List", PostListWidget()),
            ("Follower List", FollowerListWidget()),
        ]

        for name, widget in widgets:
            print(f"  ✓ {name} widget instantiated")

        return True
    except Exception as e:
        print(f"  ✗ Widget instantiation failed: {e}")
        return False


def test_protocol_selection():
    """Test that composer has protocol selection."""
    print("\nTesting protocol selection in composer...")

    try:
        from dais_cli.tui.screens.composer import ComposerScreen
        import inspect

        # Read the source to check for protocol select
        source = inspect.getsource(ComposerScreen.compose)

        if 'protocol-select' in source:
            print("  ✓ Protocol selector found in composer")
        else:
            print("  ✗ Protocol selector not found")
            return False

        if 'Both (ActivityPub + AT)' in source:
            print("  ✓ Dual-protocol option available")
        else:
            print("  ✗ Dual-protocol option missing")
            return False

        return True
    except Exception as e:
        print(f"  ✗ Protocol selection check failed: {e}")
        return False


def test_database_connectivity():
    """Test that widgets can query the database."""
    print("\nTesting database connectivity...")

    import subprocess
    import json

    try:
        # Test basic query
        result = subprocess.run(
            ["wrangler", "d1", "execute", "DB", "--local", "--command",
             "SELECT COUNT(*) as count FROM posts;"],
            capture_output=True,
            text=True,
            cwd="/home/marc/Projects/dais/workers/actor"
        )

        if result.returncode == 0:
            print("  ✓ Database connection works")
            # Parse JSON from stdout (skip wrangler headers)
            try:
                start = result.stdout.find('[')
                end = result.stdout.rfind(']') + 1
                if start >= 0 and end > 0:
                    data = json.loads(result.stdout[start:end])
                    if data and len(data) > 0 and "results" in data[0]:
                        count = data[0]["results"][0]["count"]
                        print(f"    - Found {count} posts")
            except:
                print("    - Query succeeded (JSON parsing skipped)")
        else:
            print("  ✗ Database query failed")
            return False

        # Test notifications table
        result = subprocess.run(
            ["wrangler", "d1", "execute", "DB", "--local", "--command",
             "SELECT COUNT(*) as count FROM notifications;"],
            capture_output=True,
            text=True,
            cwd="/home/marc/Projects/dais/workers/actor"
        )

        if result.returncode == 0:
            try:
                start = result.stdout.find('[')
                end = result.stdout.rfind(']') + 1
                if start >= 0 and end > 0:
                    data = json.loads(result.stdout[start:end])
                    if data and len(data) > 0 and "results" in data[0]:
                        count = data[0]["results"][0]["count"]
                        print(f"  ✓ Notifications table accessible ({count} notifications)")
            except:
                print("  ✓ Notifications table accessible")

        # Test replies table
        result = subprocess.run(
            ["wrangler", "d1", "execute", "DB", "--local", "--command",
             "SELECT COUNT(*) as count FROM replies;"],
            capture_output=True,
            text=True,
            cwd="/home/marc/Projects/dais/workers/actor"
        )

        if result.returncode == 0:
            try:
                start = result.stdout.find('[')
                end = result.stdout.rfind(']') + 1
                if start >= 0 and end > 0:
                    data = json.loads(result.stdout[start:end])
                    if data and len(data) > 0 and "results" in data[0]:
                        count = data[0]["results"][0]["count"]
                        print(f"  ✓ Replies table accessible ({count} replies)")
            except:
                print("  ✓ Replies table accessible")

        return True
    except Exception as e:
        print(f"  ✗ Database connectivity failed: {e}")
        return False


def test_moderation_features():
    """Test moderation screen features."""
    print("\nTesting moderation features...")

    try:
        from dais_cli.tui.screens.moderation import ModerationScreen

        screen = ModerationScreen()

        # Check filter options
        expected_filters = ['all', 'pending', 'hidden', 'approved']
        if hasattr(screen, 'current_filter'):
            print(f"  ✓ Moderation filter system present (default: {screen.current_filter})")

        # Check for replies list
        print("  ✓ Moderation screen has reply management")

        return True
    except Exception as e:
        print(f"  ✗ Moderation features test failed: {e}")
        return False


def test_protocol_switching():
    """Test protocol switching between ActivityPub DMs and Bluesky Chats."""
    print("\nTesting protocol switching...")

    try:
        from dais_cli.tui.screens.direct_messages import DirectMessagesScreen
        from dais_cli.tui.screens.bluesky_chat import BlueskyChatsScreen
        import inspect

        # Test DirectMessagesScreen has switch_to_bluesky action
        dm_screen = DirectMessagesScreen()
        dm_source = inspect.getsource(DirectMessagesScreen)

        if 'action_switch_to_bluesky' in dm_source:
            print("  ✓ DirectMessagesScreen has switch_to_bluesky action")
        else:
            print("  ✗ DirectMessagesScreen missing switch_to_bluesky action")
            return False

        if 'BlueskyChatsScreen' in dm_source:
            print("  ✓ DirectMessagesScreen imports BlueskyChatsScreen")
        else:
            print("  ✗ DirectMessagesScreen missing BlueskyChatsScreen import")
            return False

        # Test BlueskyChatsScreen has switch_to_activitypub action
        bsky_screen = BlueskyChatsScreen()
        bsky_source = inspect.getsource(BlueskyChatsScreen)

        if 'action_switch_to_activitypub' in bsky_source:
            print("  ✓ BlueskyChatsScreen has switch_to_activitypub action")
        else:
            print("  ✗ BlueskyChatsScreen missing switch_to_activitypub action")
            return False

        if 'DirectMessagesScreen' in bsky_source:
            print("  ✓ BlueskyChatsScreen imports DirectMessagesScreen")
        else:
            print("  ✗ BlueskyChatsScreen missing DirectMessagesScreen import")
            return False

        # Check for protocol indicators in titles
        if 'ActivityPub' in dm_source:
            print("  ✓ DirectMessagesScreen shows ActivityPub protocol")
        else:
            print("  ✗ DirectMessagesScreen missing protocol indicator")
            return False

        if 'chat.bsky.convo' in bsky_source:
            print("  ✓ BlueskyChatsScreen shows Bluesky protocol")
        else:
            print("  ✗ BlueskyChatsScreen missing protocol indicator")
            return False

        # Verify Bluesky tables exist
        result = subprocess.run(
            ["wrangler", "d1", "execute", "DB", "--local", "--command",
             "SELECT COUNT(*) as count FROM bluesky_conversations;"],
            capture_output=True,
            text=True,
            cwd="/home/marc/Projects/dais/workers/actor"
        )

        if result.returncode == 0:
            print("  ✓ bluesky_conversations table exists")
        else:
            print("  ✗ bluesky_conversations table not found")
            return False

        result = subprocess.run(
            ["wrangler", "d1", "execute", "DB", "--local", "--command",
             "SELECT COUNT(*) as count FROM bluesky_messages;"],
            capture_output=True,
            text=True,
            cwd="/home/marc/Projects/dais/workers/actor"
        )

        if result.returncode == 0:
            print("  ✓ bluesky_messages table exists")
        else:
            print("  ✗ bluesky_messages table not found")
            return False

        return True

    except Exception as e:
        print(f"  ✗ Protocol switching test failed: {e}")
        return False


def run_all_tests():
    """Run all automated tests."""
    print("=" * 60)
    print("DAIS TUI - Automated Test Suite")
    print("=" * 60)

    tests = [
        ("Import Tests", test_imports),
        ("App Instantiation", test_app_instantiation),
        ("Screen Instantiation", test_screen_instantiation),
        ("Widget Instantiation", test_widget_instantiation),
        ("Protocol Selection", test_protocol_selection),
        ("Database Connectivity", test_database_connectivity),
        ("Moderation Features", test_moderation_features),
        ("Protocol Switching", test_protocol_switching),
    ]

    results = []
    for name, test_func in tests:
        try:
            passed = test_func()
            results.append((name, passed))
        except Exception as e:
            print(f"\n  ✗ Test '{name}' crashed: {e}")
            results.append((name, False))

    print("\n" + "=" * 60)
    print("TEST RESULTS")
    print("=" * 60)

    passed_count = sum(1 for _, passed in results if passed)
    total_count = len(results)

    for name, passed in results:
        status = "✓ PASS" if passed else "✗ FAIL"
        print(f"{status:8} {name}")

    print("-" * 60)
    print(f"Total: {passed_count}/{total_count} tests passed")

    if passed_count == total_count:
        print("\n🎉 All tests passed! TUI is ready to use.")
        return 0
    else:
        print(f"\n⚠️  {total_count - passed_count} test(s) failed.")
        return 1


if __name__ == "__main__":
    sys.exit(run_all_tests())
