-- Private Mode M3: friends are mutual approved/accepted follows.

DROP VIEW IF EXISTS friends;

CREATE VIEW friends AS
SELECT
    followers.actor_id AS local_actor_id,
    followers.follower_actor_id AS friend_actor_id,
    followers.follower_inbox AS friend_inbox,
    followers.follower_shared_inbox AS friend_shared_inbox,
    followers.created_at AS follower_since,
    following.created_at AS following_since,
    following.accepted_at AS accepted_at
FROM followers
JOIN following
    ON following.actor_id = followers.actor_id
   AND following.target_actor_id = followers.follower_actor_id
WHERE followers.status = 'approved'
  AND following.status = 'accepted';
