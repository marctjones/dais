use crate::error::{CoreError, CoreResult};
use crate::traits::DatabaseProvider;
use serde_json::Value;
use url::Url;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ReadPolicy {
    Public,
    FollowersOnly,
    Private,
}

pub const E2EE_FALLBACK_MARKER: &str = "End-to-end encrypted message";

pub const ANONYMOUS_PUBLIC_POST_SQL_PREDICATE: &str =
    "visibility = 'public' AND encrypted_message IS NULL AND content NOT LIKE '%End-to-end encrypted message%'";

pub fn read_policy_from_visibility(visibility: &str) -> ReadPolicy {
    match visibility {
        "public" | "unlisted" => ReadPolicy::Public,
        "followers" => ReadPolicy::FollowersOnly,
        "direct" => ReadPolicy::Private,
        _ => ReadPolicy::FollowersOnly,
    }
}

pub fn requires_authorized_fetch(visibility: &str) -> bool {
    matches!(
        read_policy_from_visibility(visibility),
        ReadPolicy::FollowersOnly | ReadPolicy::Private
    )
}

pub fn is_anonymous_public_post(
    visibility: &str,
    encrypted_message: Option<&str>,
    content: &str,
) -> bool {
    visibility == "public" && encrypted_message.is_none() && !content.contains(E2EE_FALLBACK_MARKER)
}

pub fn requires_authorized_post_fetch(
    visibility: &str,
    encrypted_message: Option<&str>,
    content: &str,
) -> bool {
    requires_authorized_fetch(visibility)
        || encrypted_message.is_some()
        || content.contains(E2EE_FALLBACK_MARKER)
}

pub fn can_fetch_post(
    visibility: &str,
    encrypted_message: Option<&str>,
    content: &str,
    requester_is_approved_follower: bool,
) -> bool {
    if is_anonymous_public_post(visibility, encrypted_message, content) {
        return true;
    }
    match read_policy_from_visibility(visibility) {
        ReadPolicy::Public => false,
        ReadPolicy::FollowersOnly => {
            requester_is_approved_follower
                && encrypted_message.is_none()
                && !content.contains(E2EE_FALLBACK_MARKER)
        }
        ReadPolicy::Private => false,
    }
}

pub async fn is_blocked_actor(db: &dyn DatabaseProvider, actor_url: &str) -> CoreResult<bool> {
    let actor_query = "SELECT COUNT(*) AS count FROM blocks WHERE actor_id = ?1";
    let actor_rows = db
        .execute(actor_query, &[Value::String(actor_url.to_string())])
        .await?;
    if let Some(count) = actor_rows
        .first()
        .and_then(|row| row.get("count"))
        .and_then(|value| value.as_u64())
    {
        if count > 0 {
            return Ok(true);
        }
    }

    let Some(domain) = actor_domain(actor_url) else {
        return Ok(false);
    };

    let domain_query = "SELECT COUNT(*) AS count FROM blocks WHERE blocked_domain = ?1";
    let domain_rows = db.execute(domain_query, &[Value::String(domain)]).await?;
    Ok(domain_rows
        .first()
        .and_then(|row| row.get("count"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0)
        > 0)
}

pub async fn is_approved_follower(db: &dyn DatabaseProvider, actor_url: &str) -> CoreResult<bool> {
    let query = "SELECT COUNT(*) as count FROM followers WHERE follower_actor_id = ?1 AND status = 'approved'";
    let rows = db
        .execute(query, &[Value::String(actor_url.to_string())])
        .await?;
    if let Some(count) = rows
        .first()
        .and_then(|row| row.get("count"))
        .and_then(|value| value.as_u64())
    {
        return Ok(count > 0);
    }
    Ok(false)
}

pub async fn is_closed_network_enabled(db: &dyn DatabaseProvider) -> CoreResult<bool> {
    let query = "SELECT closed_network FROM instance_settings WHERE id = 1";
    let rows = match db.execute(query, &[]).await {
        Ok(rows) => rows,
        Err(_) => return Ok(false),
    };

    Ok(rows
        .first()
        .and_then(|row| row.get("closed_network"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        == 1)
}

pub async fn is_federation_host_allowed(
    db: &dyn DatabaseProvider,
    remote_url: &str,
) -> CoreResult<bool> {
    if !is_closed_network_enabled(db).await? {
        return Ok(true);
    }

    let Some(host) = actor_domain(remote_url) else {
        return Ok(false);
    };

    let query = r#"
        SELECT COUNT(*) AS count
        FROM federation_allowlist
        WHERE host = ?1
          AND enabled = 1
    "#;
    let rows = db.execute(query, &[Value::String(host)]).await?;
    Ok(rows
        .first()
        .and_then(|row| row.get("count"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0)
}

pub fn require_https_url(value: &str) -> CoreResult<Url> {
    let url = Url::parse(value)
        .map_err(|_| CoreError::InvalidActivity(format!("Invalid actor URL: {}", value)))?;
    if url.scheme() != "https" {
        return Err(CoreError::InvalidActivity(format!(
            "Actor URL must use https: {}",
            value
        )));
    }
    Ok(url)
}

fn actor_domain(actor_url: &str) -> Option<String> {
    require_https_url(actor_url)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_string()))
}

#[cfg(test)]
fn blocked_domain_matches_actor(blocked_domain: &str, actor_url: &str) -> bool {
    actor_domain(actor_url)
        .map(|domain| domain == blocked_domain)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{
        blocked_domain_matches_actor, can_fetch_post, is_anonymous_public_post,
        read_policy_from_visibility, requires_authorized_fetch, requires_authorized_post_fetch,
        ReadPolicy,
    };

    #[test]
    fn public_and_unlisted_are_public() {
        assert_eq!(read_policy_from_visibility("public"), ReadPolicy::Public);
        assert_eq!(read_policy_from_visibility("unlisted"), ReadPolicy::Public);
        assert!(!requires_authorized_fetch("public"));
        assert!(!requires_authorized_fetch("unlisted"));
    }

    #[test]
    fn followers_and_direct_require_authorized_fetch() {
        assert_eq!(
            read_policy_from_visibility("followers"),
            ReadPolicy::FollowersOnly
        );
        assert_eq!(read_policy_from_visibility("direct"), ReadPolicy::Private);
        assert!(requires_authorized_fetch("followers"));
        assert!(requires_authorized_fetch("direct"));
    }

    #[test]
    fn anonymous_public_posts_exclude_private_and_encrypted_fallbacks() {
        assert!(is_anonymous_public_post("public", None, "hello"));
        assert!(!is_anonymous_public_post("unlisted", None, "hello"));
        assert!(!is_anonymous_public_post("followers", None, "hello"));
        assert!(!is_anonymous_public_post("direct", None, "hello"));
        assert!(!is_anonymous_public_post(
            "public",
            Some(r#"{"v":1}"#),
            "hello"
        ));
        assert!(!is_anonymous_public_post(
            "public",
            None,
            "End-to-end encrypted message"
        ));
    }

    #[test]
    fn authorized_post_fetch_includes_visibility_and_legacy_encrypted_rows() {
        assert!(!requires_authorized_post_fetch("public", None, "hello"));
        assert!(requires_authorized_post_fetch("followers", None, "hello"));
        assert!(requires_authorized_post_fetch("direct", None, "hello"));
        assert!(requires_authorized_post_fetch(
            "public",
            Some(r#"{"v":1}"#),
            "hello"
        ));
        assert!(requires_authorized_post_fetch(
            "public",
            None,
            "End-to-end encrypted message"
        ));
    }

    #[test]
    fn post_fetch_policy_allows_only_intended_readers() {
        assert!(can_fetch_post("public", None, "hello", false));
        assert!(can_fetch_post("public", None, "hello", true));
        assert!(!can_fetch_post("followers", None, "hello", false));
        assert!(can_fetch_post("followers", None, "hello", true));
        assert!(!can_fetch_post("direct", None, "hello", true));
        assert!(!can_fetch_post("public", Some(r#"{"v":1}"#), "fallback", true));
        assert!(!can_fetch_post(
            "public",
            None,
            "End-to-end encrypted message",
            true
        ));
    }

    #[test]
    fn blocked_domains_match_actor_hosts() {
        assert!(blocked_domain_matches_actor(
            "mastodon.example",
            "https://mastodon.example/users/alice"
        ));
        assert!(!blocked_domain_matches_actor(
            "mastodon.example",
            "https://social.example/users/alice"
        ));
    }
}
