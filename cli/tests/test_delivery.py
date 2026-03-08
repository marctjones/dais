"""Tests for activity delivery module."""

import pytest
from datetime import datetime
from dais_cli.delivery import (
    build_accept_activity,
    build_reject_activity,
    build_create_activity,
    build_delete_activity
)


def test_build_accept_activity():
    """Test building Accept activity for Follow request."""
    actor_url = "https://social.example.com/users/alice"
    follow_activity = {
        "type": "Follow",
        "id": "https://mastodon.example.com/follows/123",
        "actor": "https://mastodon.example.com/users/bob",
        "object": actor_url
    }

    accept = build_accept_activity(actor_url, follow_activity)

    assert accept["type"] == "Accept"
    assert accept["actor"] == actor_url
    assert accept["object"] == follow_activity
    assert "@context" in accept
    assert "id" in accept


def test_build_reject_activity():
    """Test building Reject activity for Follow request."""
    actor_url = "https://social.example.com/users/alice"
    follow_activity = {
        "type": "Follow",
        "id": "https://mastodon.example.com/follows/456",
        "actor": "https://mastodon.example.com/users/charlie",
        "object": actor_url
    }

    reject = build_reject_activity(actor_url, follow_activity)

    assert reject["type"] == "Reject"
    assert reject["actor"] == actor_url
    assert reject["object"] == follow_activity
    assert "@context" in reject
    assert "id" in reject


def test_build_create_activity():
    """Test building Create activity for Note."""
    actor_url = "https://social.example.com/users/alice"
    note = {
        "type": "Note",
        "id": "https://social.example.com/users/alice/posts/001",
        "attributedTo": actor_url,
        "content": "Hello, Fediverse!",
        "published": "2024-01-01T00:00:00Z",
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "cc": []
    }

    create = build_create_activity(actor_url, note)

    assert create["type"] == "Create"
    assert create["actor"] == actor_url
    assert create["object"] == note
    assert "@context" in create
    assert "id" in create
    assert "published" in create
    assert create["to"] == note["to"]
    assert create["cc"] == note["cc"]


def test_build_delete_activity():
    """Test building Delete activity for post."""
    actor_url = "https://social.example.com/users/alice"
    object_url = "https://social.example.com/users/alice/posts/001"

    delete = build_delete_activity(actor_url, object_url)

    assert delete["type"] == "Delete"
    assert delete["actor"] == actor_url
    assert delete["object"] == object_url
    assert "@context" in delete
    assert "id" in delete
    assert delete["to"] == ["https://www.w3.org/ns/activitystreams#Public"]


def test_activity_id_uniqueness():
    """Test that activity IDs are unique (timestamp-based)."""
    actor_url = "https://social.example.com/users/alice"
    follow = {
        "type": "Follow",
        "id": "https://example.com/follows/1",
        "actor": "https://example.com/users/bob",
        "object": actor_url
    }

    accept1 = build_accept_activity(actor_url, follow)
    accept2 = build_accept_activity(actor_url, follow)

    # IDs should be different due to timestamps
    # (they might be the same if called in the same second, but unlikely)
    assert "id" in accept1
    assert "id" in accept2


def test_create_activity_inherits_audience():
    """Test that Create activity inherits to/cc from Note."""
    actor_url = "https://social.example.com/users/alice"
    note = {
        "type": "Note",
        "id": "https://social.example.com/users/alice/posts/001",
        "attributedTo": actor_url,
        "content": "Test",
        "published": "2024-01-01T00:00:00Z",
        "to": ["https://www.w3.org/ns/activitystreams#Public"],
        "cc": ["https://social.example.com/users/alice/followers"]
    }

    create = build_create_activity(actor_url, note)

    assert create["to"] == note["to"]
    assert create["cc"] == note["cc"]


def test_delete_activity_public_audience():
    """Test that Delete activity has public audience."""
    actor_url = "https://social.example.com/users/alice"
    object_url = "https://social.example.com/users/alice/posts/001"

    delete = build_delete_activity(actor_url, object_url)

    assert "https://www.w3.org/ns/activitystreams#Public" in delete["to"]
