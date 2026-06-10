use crate::atproto::{FeedItem, Profile};
use crate::d1::{D1Friend, D1Post, D1TimelinePost, D1User, ServerStats};

pub fn print_profile(profile: &Profile) {
    println!("@{}", profile.handle);
    if let Some(display_name) = &profile.display_name {
        if !display_name.is_empty() {
            println!("{display_name}");
        }
    }
    println!("DID: {}", profile.did);
    if let Some(description) = &profile.description {
        if !description.is_empty() {
            println!();
            println!("{description}");
        }
    }
    println!();
    println!(
        "posts={} followers={} following={}",
        profile.posts_count.unwrap_or(0),
        profile.followers_count.unwrap_or(0),
        profile.follows_count.unwrap_or(0)
    );
}

pub fn print_profiles(profiles: &[Profile]) {
    if profiles.is_empty() {
        println!("No accounts found");
        return;
    }

    for profile in profiles {
        let display_name = profile.display_name.as_deref().unwrap_or("");
        if display_name.is_empty() {
            println!("@{} ({})", profile.handle, profile.did);
        } else {
            println!("@{} - {} ({})", profile.handle, display_name, profile.did);
        }
    }
}

pub fn print_feed(feed: &[FeedItem]) {
    if feed.is_empty() {
        println!("No posts found");
        return;
    }

    for item in feed {
        let post = &item.post;
        let text = post.record.text.as_deref().unwrap_or("");
        let created_at = post.record.created_at.as_deref().unwrap_or("unknown time");
        let display_name = post
            .author
            .display_name
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or(&post.author.handle);

        if item.reason.is_some() {
            println!("repost");
        }
        println!("@{} ({display_name}) - {created_at}", post.author.handle);
        println!("{text}");
        println!(
            "replies={} reposts={} likes={} uri={}{}",
            post.reply_count.unwrap_or(0),
            post.repost_count.unwrap_or(0),
            post.like_count.unwrap_or(0),
            post.uri,
            post.cid
                .as_deref()
                .map(|cid| format!(" cid={cid}"))
                .unwrap_or_default()
        );
        println!();
    }
}

pub fn print_posts(posts: &[D1Post]) {
    if posts.is_empty() {
        println!("No posts found");
        return;
    }

    for post in posts {
        println!(
            "{} [{} / {}{}]",
            post.published_at.as_deref().unwrap_or("unknown time"),
            post.visibility.as_deref().unwrap_or("unknown"),
            post.protocol.as_deref().unwrap_or("activitypub"),
            post.encrypted_message
                .as_ref()
                .map(|_| " / encrypted")
                .unwrap_or("")
        );
        println!("{}", post.content);
        println!("id={}", post.id);
        if let Some(reply_to) = post
            .in_reply_to
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            println!("reply_to={reply_to}");
        }
        if let Some(uri) = &post.atproto_uri {
            if !uri.is_empty() {
                println!("atproto={uri}");
            }
        }
        println!();
    }
}

pub fn print_users(users: &[D1User]) {
    if users.is_empty() {
        println!("No users found");
        return;
    }

    for user in users {
        println!(
            "{} [{} / {}] {}",
            user.actor_id,
            user.relation,
            user.status,
            user.created_at.as_deref().unwrap_or("")
        );
    }
}

pub fn print_timeline(posts: &[D1TimelinePost]) {
    if posts.is_empty() {
        println!("No timeline posts found");
        return;
    }

    for post in posts {
        let display_name = post
            .actor_display_name
            .as_deref()
            .filter(|name| !name.is_empty())
            .or(post.actor_username.as_deref())
            .unwrap_or(&post.actor_id);
        println!(
            "{} [{} / {}{}]",
            post.published_at.as_deref().unwrap_or("unknown time"),
            post.visibility.as_deref().unwrap_or("unknown"),
            post.protocol.as_deref().unwrap_or("activitypub"),
            post.encrypted_message
                .as_ref()
                .map(|_| " / encrypted")
                .unwrap_or("")
        );
        println!("{display_name} - {}", post.actor_id);
        println!("{}", post.content);
        println!("id={}", post.object_id);
        if let Some(updated_at) = post.updated_at.as_deref().filter(|value| !value.is_empty()) {
            println!("updated={updated_at}");
        }
        println!();
    }
}

pub fn print_friends(friends: &[D1Friend]) {
    if friends.is_empty() {
        println!("No friends found");
        return;
    }

    for friend in friends {
        println!("{}", friend.friend_actor_id);
        if let Some(inbox) = friend
            .friend_inbox
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            println!("inbox={inbox}");
        }
        if let Some(shared) = friend
            .friend_shared_inbox
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            println!("shared_inbox={shared}");
        }
        println!(
            "follower_since={} following_since={} accepted_at={}",
            friend.follower_since.as_deref().unwrap_or(""),
            friend.following_since.as_deref().unwrap_or(""),
            friend.accepted_at.as_deref().unwrap_or("")
        );
        println!();
    }
}

pub fn print_server_stats(stats: &ServerStats, remote: bool) {
    println!(
        "Database: {}",
        if remote {
            "remote production"
        } else {
            "local development"
        }
    );
    println!("followers total={}", stats.followers_total);
    println!("followers approved={}", stats.followers_approved);
    println!("followers pending={}", stats.followers_pending);
    println!("followers rejected={}", stats.followers_rejected);
    println!("following total={}", stats.following_total);
    println!("posts total={}", stats.posts_total);
    println!("posts dual_protocol={}", stats.dual_protocol_posts);
    println!("activities total={}", stats.activities_total);
    println!("deliveries total={}", stats.deliveries_total);
    println!("deliveries failed={}", stats.deliveries_failed);
}
