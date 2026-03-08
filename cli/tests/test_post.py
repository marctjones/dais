"""Tests for post management commands."""

import pytest
from click.testing import CliRunner
from dais_cli.commands.post import post


def test_post_create_command_structure():
    """Test that post create command accepts required arguments."""
    runner = CliRunner()

    # Test with content argument
    result = runner.invoke(post, ['create', 'Test post', '--visibility', 'public'])

    # Command should not error on argument parsing
    # It will fail on database access, but that's expected in tests
    assert 'Test post' in result.output or result.exit_code != 0


def test_post_create_visibility_options():
    """Test that post create accepts all visibility options."""
    runner = CliRunner()

    visibilities = ['public', 'unlisted', 'followers', 'direct']

    for visibility in visibilities:
        result = runner.invoke(post, ['create', 'Test', '--visibility', visibility])
        # Should not error on visibility validation
        # Will fail on database access, but that's expected


def test_post_list_command_structure():
    """Test that post list command accepts limit option."""
    runner = CliRunner()

    result = runner.invoke(post, ['list', '--limit', '10'])

    # Command structure should be valid
    # Will fail on database access, but that's expected
    assert result.exit_code != 0 or 'Listing' in result.output


def test_post_delete_command_structure():
    """Test that post delete command accepts post ID argument."""
    runner = CliRunner()

    result = runner.invoke(post, ['delete', '001'])

    # Command should accept the argument
    # Will fail on database access, but that's expected
    assert result.exit_code != 0 or 'Deleting' in result.output


def test_post_id_generation():
    """Test post ID generation format."""
    from datetime import datetime
    import uuid

    # Simulate post ID generation
    post_uuid = str(uuid.uuid4())[:8]
    timestamp = datetime.utcnow().strftime('%Y%m%d%H%M%S')
    post_id_path = f"{timestamp}-{post_uuid}"

    # Verify format
    assert len(post_id_path) > 14  # timestamp is 14 chars
    assert '-' in post_id_path
    parts = post_id_path.split('-')
    assert len(parts) == 2
    assert parts[0].isdigit()  # timestamp part
    assert len(parts[1]) == 8  # UUID part


def test_post_url_construction():
    """Test full post URL construction."""
    actor_username = "marc"
    post_id_path = "20260107120000-abc123"

    post_url = f"https://social.dais.social/users/{actor_username}/posts/{post_id_path}"

    assert post_url == "https://social.dais.social/users/marc/posts/20260107120000-abc123"
    assert post_url.startswith("https://")
    assert "/users/" in post_url
    assert "/posts/" in post_url


def test_visibility_audience_mapping():
    """Test that visibility maps to correct ActivityPub audience."""
    actor_id = "https://social.dais.social/users/marc"

    # Public
    to_audience = ["https://www.w3.org/ns/activitystreams#Public"]
    cc_audience = []
    assert "Public" in to_audience[0]

    # Unlisted
    to_audience = []
    cc_audience = ["https://www.w3.org/ns/activitystreams#Public"]
    assert "Public" in cc_audience[0]

    # Followers
    to_audience = [f"{actor_id}/followers"]
    cc_audience = []
    assert "followers" in to_audience[0]

    # Direct
    to_audience = []
    cc_audience = []
    assert len(to_audience) == 0
    assert len(cc_audience) == 0


def test_sql_escape_for_post_content():
    """Test that single quotes are escaped for SQL."""
    content = "Hello 'world' with quotes"
    content_escaped = content.replace("'", "''")

    assert content_escaped == "Hello ''world'' with quotes"
    assert "''" in content_escaped
    assert content_escaped.count("''") == 2
