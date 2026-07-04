pub(crate) fn visibility(value: &str) -> &'static str {
    match value {
        "public" => "public",
        "unlisted" => "unlisted",
        "direct" => "direct",
        _ => "private",
    }
}

pub(crate) fn follow_request_action(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/v1/follow_requests/") else {
        return false;
    };
    let mut parts = rest.split('/');
    let Some(id) = parts.next() else {
        return false;
    };
    !id.is_empty() && matches!(parts.next(), Some("authorize" | "reject")) && parts.next().is_none()
}

pub(crate) fn suggestion_dismiss(path: &str) -> bool {
    path.strip_prefix("/api/v1/suggestions/")
        .map(|rest| !rest.is_empty() && !rest.contains('/'))
        .unwrap_or(false)
}

pub(crate) fn account_statuses_path(path: &str) -> bool {
    account_collection_path(path, "statuses")
}

pub(crate) fn account_followers_path(path: &str) -> bool {
    account_collection_path(path, "followers")
}

pub(crate) fn account_following_path(path: &str) -> bool {
    account_collection_path(path, "following")
}

fn account_collection_path(path: &str, collection: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/v1/accounts/") else {
        return false;
    };
    let mut parts = rest.split('/');
    let Some(id) = parts.next() else {
        return false;
    };
    !id.is_empty() && parts.next() == Some(collection) && parts.next().is_none()
}

pub(crate) fn account_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/v1/accounts/") else {
        return false;
    };
    !rest.is_empty() && !rest.contains('/')
}

pub(crate) fn account_action_path(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("/api/v1/accounts/")?;
    let mut parts = rest.split('/');
    let id = parts.next()?;
    let action = parts.next()?;
    if id.is_empty()
        || parts.next().is_some()
        || !matches!(
            action,
            "follow" | "unfollow" | "block" | "unblock" | "mute" | "unmute"
        )
    {
        return None;
    }
    Some((id.to_string(), action.to_string()))
}

pub(crate) fn status_context_path(path: &str) -> Option<String> {
    status_subpath(path, "context")
}

pub(crate) fn status_source_path(path: &str) -> Option<String> {
    status_subpath(path, "source")
}

pub(crate) fn status_action_path(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("/api/v1/statuses/")?;
    for action in ["favourite", "unfavourite", "reblog", "unreblog"] {
        let suffix = format!("/{action}");
        if let Some(id) = rest.strip_suffix(&suffix).filter(|id| !id.is_empty()) {
            return Some((id.to_string(), action.to_string()));
        }
    }
    None
}

fn status_subpath(path: &str, suffix: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/v1/statuses/")?;
    let needle = format!("/{suffix}");
    let id = rest.strip_suffix(&needle)?;
    (!id.is_empty()).then(|| id.to_string())
}

pub(crate) fn status_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/v1/statuses/")?;
    (!rest.is_empty() && !rest.contains('/')).then(|| rest.to_string())
}

pub(crate) fn media_path(path: &str) -> Option<String> {
    path.strip_prefix("/api/v1/media/")
        .or_else(|| path.strip_prefix("/api/v2/media/"))
        .filter(|rest| !rest.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn notification_dismiss_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/api/v1/notifications/")?;
    let id = rest.strip_suffix("/dismiss")?;
    (!id.is_empty()).then(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_account_collections_without_accepting_extra_segments() {
        assert!(account_statuses_path("/api/v1/accounts/alice/statuses"));
        assert!(account_followers_path("/api/v1/accounts/alice/followers"));
        assert!(account_following_path("/api/v1/accounts/alice/following"));
        assert!(!account_statuses_path(
            "/api/v1/accounts/alice/statuses/more"
        ));
        assert!(!account_statuses_path("/api/v1/accounts//statuses"));
    }

    #[test]
    fn parses_account_actions_allowlist() {
        assert_eq!(
            account_action_path("/api/v1/accounts/bob/mute"),
            Some(("bob".to_string(), "mute".to_string()))
        );
        assert_eq!(
            account_action_path("/api/v1/accounts/bob/unfollow"),
            Some(("bob".to_string(), "unfollow".to_string()))
        );
        assert_eq!(account_action_path("/api/v1/accounts/bob/admin"), None);
        assert_eq!(account_action_path("/api/v1/accounts/bob/mute/extra"), None);
    }

    #[test]
    fn parses_status_paths_and_actions() {
        assert_eq!(status_path("/api/v1/statuses/123"), Some("123".to_string()));
        assert_eq!(
            status_context_path("/api/v1/statuses/123/context"),
            Some("123".to_string())
        );
        assert_eq!(
            status_source_path("/api/v1/statuses/123/source"),
            Some("123".to_string())
        );
        assert_eq!(
            status_action_path("/api/v1/statuses/123/favourite"),
            Some(("123".to_string(), "favourite".to_string()))
        );
        assert_eq!(status_path("/api/v1/statuses/123/context"), None);
        assert_eq!(status_action_path("/api/v1/statuses/123/delete"), None);
    }

    #[test]
    fn parses_misc_client_probe_paths() {
        assert!(follow_request_action(
            "/api/v1/follow_requests/abc/authorize"
        ));
        assert!(follow_request_action("/api/v1/follow_requests/abc/reject"));
        assert!(!follow_request_action("/api/v1/follow_requests/abc/ignore"));
        assert!(suggestion_dismiss("/api/v1/suggestions/example"));
        assert!(!suggestion_dismiss("/api/v1/suggestions/example/extra"));
        assert_eq!(
            media_path("/api/v2/media/media-1"),
            Some("media-1".to_string())
        );
        assert_eq!(
            notification_dismiss_path("/api/v1/notifications/n1/dismiss"),
            Some("n1".to_string())
        );
    }
}
