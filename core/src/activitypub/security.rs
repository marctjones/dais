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
        blocked_domain_matches_actor, read_policy_from_visibility, requires_authorized_fetch,
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
