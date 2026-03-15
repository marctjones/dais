"""Test script for Bluesky Reply Consumer.

Tests the consumer can load and basic functionality works.
"""

import sys
from pathlib import Path

# Add services to path
sys.path.insert(0, str(Path(__file__).parent))

def test_consumer_import():
    """Test that the consumer can be imported."""
    print("Testing consumer import...")
    try:
        from bluesky_reply_consumer import BlueskyReplyConsumer, ATPROTO_AVAILABLE
        print(f"  ✓ Consumer imported successfully")
        print(f"  ✓ atproto library available: {ATPROTO_AVAILABLE}")
        return True
    except Exception as e:
        print(f"  ✗ Failed to import consumer: {e}")
        return False

def test_consumer_creation():
    """Test that the consumer can be instantiated."""
    print("\nTesting consumer creation...")
    try:
        from bluesky_reply_consumer import BlueskyReplyConsumer
        consumer = BlueskyReplyConsumer(remote=False)
        print(f"  ✓ Consumer created successfully")
        print(f"  ✓ Worker dir: {consumer.worker_dir}")
        print(f"  ✓ Remote mode: {consumer.remote}")
        return True
    except Exception as e:
        print(f"  ✗ Failed to create consumer: {e}")
        return False

def test_load_posts():
    """Test that the consumer can load posts from database."""
    print("\nTesting post loading...")
    try:
        from bluesky_reply_consumer import BlueskyReplyConsumer
        consumer = BlueskyReplyConsumer(remote=False)
        consumer.load_our_posts()
        print(f"  ✓ Loaded {len(consumer.our_posts)} posts")
        return True
    except Exception as e:
        print(f"  ✗ Failed to load posts: {e}")
        return False

def test_extract_reply_parent():
    """Test reply parent extraction."""
    print("\nTesting reply parent extraction...")
    try:
        from bluesky_reply_consumer import BlueskyReplyConsumer, ATPROTO_AVAILABLE
        if not ATPROTO_AVAILABLE:
            print("  ⚠ Skipped (atproto not available)")
            return True

        from atproto import models

        consumer = BlueskyReplyConsumer(remote=False)

        # Create a mock reply record
        class MockReply:
            class Parent:
                uri = "at://did:plc:test/app.bsky.feed.post/123"
            parent = Parent()

        class MockRecord:
            text = "This is a test reply"
            reply = MockReply()

        record = MockRecord()
        parent_uri = consumer.extract_reply_parent(record)

        if parent_uri == "at://did:plc:test/app.bsky.feed.post/123":
            print(f"  ✓ Reply parent extracted correctly: {parent_uri}")
            return True
        else:
            print(f"  ✗ Unexpected parent URI: {parent_uri}")
            return False

    except Exception as e:
        print(f"  ✗ Failed to test reply extraction: {e}")
        return False

def test_dependencies():
    """Test that required dependencies are available."""
    print("\nTesting dependencies...")
    results = []

    # Test atproto
    try:
        import atproto
        print(f"  ✓ atproto: {atproto.__version__ if hasattr(atproto, '__version__') else 'installed'}")
        results.append(True)
    except ImportError:
        print("  ✗ atproto: NOT INSTALLED (pip install atproto)")
        results.append(False)

    # Test websockets
    try:
        import websockets
        print(f"  ✓ websockets: installed")
        results.append(True)
    except ImportError:
        print("  ✗ websockets: NOT INSTALLED (pip install websockets)")
        results.append(False)

    # Test cbor2
    try:
        import cbor2
        print(f"  ✓ cbor2: installed")
        results.append(True)
    except ImportError:
        print("  ✗ cbor2: NOT INSTALLED (pip install cbor2)")
        results.append(False)

    return all(results)

def run_all_tests():
    """Run all tests."""
    print("=" * 60)
    print("Bluesky Reply Consumer - Test Suite")
    print("=" * 60)

    tests = [
        ("Dependencies", test_dependencies),
        ("Consumer Import", test_consumer_import),
        ("Consumer Creation", test_consumer_creation),
        ("Post Loading", test_load_posts),
        ("Reply Parent Extraction", test_extract_reply_parent),
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
        print("\n🎉 All tests passed! Consumer is ready to use.")
        return 0
    else:
        print(f"\n⚠️  {total_count - passed_count} test(s) failed.")
        return 1

if __name__ == "__main__":
    sys.exit(run_all_tests())
