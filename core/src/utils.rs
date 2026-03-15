/// Utility functions for dais core

use chrono::Utc;

/// Generate a unique ID for posts, activities, etc.
///
/// Format: YYYYMMDDHHMMSS-{random}
///
/// Example: "20260315231545-a1b2c3d4"
pub fn generate_id() -> String {
    let timestamp = Utc::now().format("%Y%m%d%H%M%S");
    let random = uuid::Uuid::new_v4().to_string();
    let random_short = &random[0..8]; // First 8 chars
    format!("{}-{}", timestamp, random_short)
}

/// Generate a UUID v4
pub fn generate_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Get current timestamp in RFC3339 format
///
/// Example: "2026-03-15T23:15:45Z"
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

/// Get current timestamp in ISO 8601 format with milliseconds
///
/// Example: "2026-03-15T23:15:45.123Z"
pub fn now_iso8601_millis() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// Normalize ActivityPub ID (remove trailing slashes, etc.)
pub fn normalize_id(id: &str) -> String {
    id.trim_end_matches('/').to_string()
}

/// Extract username from ActivityPub handle
///
/// Examples:
/// - "@user@example.com" -> "user"
/// - "user@example.com" -> "user"
/// - "user" -> "user"
pub fn extract_username(handle: &str) -> String {
    handle
        .trim_start_matches('@')
        .split('@')
        .next()
        .unwrap_or(handle)
        .to_string()
}

/// Extract domain from ActivityPub handle
///
/// Examples:
/// - "@user@example.com" -> Some("example.com")
/// - "user@example.com" -> Some("example.com")
/// - "user" -> None
pub fn extract_domain(handle: &str) -> Option<String> {
    let parts: Vec<&str> = handle.trim_start_matches('@').split('@').collect();
    if parts.len() > 1 {
        Some(parts[1].to_string())
    } else {
        None
    }
}

/// Build ActivityPub actor URL
///
/// Example: actor_url("social.example.com", "user") -> "https://social.example.com/users/user"
pub fn actor_url(domain: &str, username: &str) -> String {
    format!("https://{}/users/{}", domain, username)
}

/// Build post URL
///
/// Example: post_url("social.example.com", "user", "12345") -> "https://social.example.com/users/user/posts/12345"
pub fn post_url(domain: &str, username: &str, post_id: &str) -> String {
    format!("https://{}/users/{}/posts/{}", domain, username, post_id)
}

/// Validate email address (basic check)
pub fn is_valid_email(email: &str) -> bool {
    email.contains('@') && email.contains('.')
}

/// Sanitize HTML content (strip dangerous tags)
///
/// TODO: Implement proper HTML sanitization
pub fn sanitize_html(html: &str) -> String {
    // For now, just return as-is
    // In production, use ammonia or similar library
    html.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_id() {
        let id = generate_id();
        assert!(id.contains('-'));
        assert!(id.len() > 15);
    }

    #[test]
    fn test_extract_username() {
        assert_eq!(extract_username("@user@example.com"), "user");
        assert_eq!(extract_username("user@example.com"), "user");
        assert_eq!(extract_username("user"), "user");
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("@user@example.com"), Some("example.com".to_string()));
        assert_eq!(extract_domain("user@example.com"), Some("example.com".to_string()));
        assert_eq!(extract_domain("user"), None);
    }

    #[test]
    fn test_actor_url() {
        assert_eq!(
            actor_url("social.example.com", "user"),
            "https://social.example.com/users/user"
        );
    }

    #[test]
    fn test_post_url() {
        assert_eq!(
            post_url("social.example.com", "user", "12345"),
            "https://social.example.com/users/user/posts/12345"
        );
    }
}
