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
#[allow(dead_code)]
pub fn now_iso8601_millis() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// Normalize ActivityPub ID (remove trailing slashes, etc.)
#[allow(dead_code)]
pub fn normalize_id(id: &str) -> String {
    id.trim_end_matches('/').to_string()
}

/// Extract username from ActivityPub handle
///
/// Examples:
/// - "@user@example.com" -> "user"
/// - "user@example.com" -> "user"
/// - "user" -> "user"
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
pub fn is_valid_email(email: &str) -> bool {
    email.contains('@') && email.contains('.')
}

/// Sanitize HTML content for ActivityPub/owner-facing previews.
pub fn sanitize_html(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut rest = html;
    let mut drop_until: Option<&'static str> = None;
    let mut open_tags: Vec<String> = Vec::new();

    loop {
        if let Some(blocked) = drop_until {
            let closing = format!("</{blocked}");
            let lower_rest = rest.to_ascii_lowercase();
            let Some(close_start) = lower_rest.find(&closing) else {
                rest = "";
                break;
            };
            rest = &rest[close_start..];
        }

        let Some(start) = rest.find('<') else {
            break;
        };
        output.push_str(&rest[..start]);
        rest = &rest[start..];

        let Some(end) = rest.find('>') else {
            output.push_str("&lt;");
            rest = &rest[1..];
            continue;
        };

        let tag = &rest[..=end];
        rest = &rest[end + 1..];
        let lower_name = html_tag_name(tag).to_ascii_lowercase();

        if let Some(blocked) = drop_until {
            if tag_is_closing(tag) && lower_name == blocked {
                drop_until = None;
            }
            continue;
        }

        if matches!(
            lower_name.as_str(),
            "script" | "style" | "iframe" | "object" | "embed"
        ) {
            if !tag_is_closing(tag) {
                drop_until = Some(match lower_name.as_str() {
                    "script" => "script",
                    "style" => "style",
                    "iframe" => "iframe",
                    "object" => "object",
                    "embed" => "embed",
                    _ => unreachable!(),
                });
            }
            continue;
        }

        if tag_is_closing(tag) {
            if allowed_html_tag(&lower_name)
                && open_tags
                    .iter()
                    .rposition(|open| open == &lower_name)
                    .is_some()
            {
                let position = open_tags
                    .iter()
                    .rposition(|open| open == &lower_name)
                    .expect("position checked above");
                open_tags.truncate(position);
                output.push_str("</");
                output.push_str(&lower_name);
                output.push('>');
            }
            continue;
        }

        if lower_name == "a" {
            if let Some(href) = safe_href(tag) {
                output.push_str("<a href=\"");
                output.push_str(&escape_html_attr(&href));
                output.push_str("\">");
                open_tags.push(lower_name);
            }
        } else if allowed_html_tag(&lower_name) {
            output.push('<');
            output.push_str(&lower_name);
            output.push('>');
            open_tags.push(lower_name);
        }
    }

    if drop_until.is_none() {
        output.push_str(rest);
    }
    output
}

fn html_tag_name(tag: &str) -> &str {
    tag.trim_start_matches('<')
        .trim_start_matches('/')
        .trim_start()
        .split(|ch: char| ch.is_ascii_whitespace() || ch == '>' || ch == '/')
        .next()
        .unwrap_or("")
}

fn tag_is_closing(tag: &str) -> bool {
    tag.trim_start_matches('<').trim_start().starts_with('/')
}

fn allowed_html_tag(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "br"
            | "strong"
            | "b"
            | "em"
            | "i"
            | "code"
            | "pre"
            | "blockquote"
            | "ul"
            | "ol"
            | "li"
            | "span"
            | "a"
    )
}

fn safe_href(tag: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let href_pos = lower.find("href")?;
    let after = &tag[href_pos + 4..];
    let after = after.trim_start();
    let after = after.strip_prefix('=')?.trim_start();
    let value = if let Some(quoted) = after.strip_prefix('"') {
        let end = quoted.find('"')?;
        &quoted[..end]
    } else if let Some(quoted) = after.strip_prefix('\'') {
        let end = quoted.find('\'')?;
        &quoted[..end]
    } else {
        let end = after
            .find(|ch: char| ch.is_ascii_whitespace() || ch == '>')
            .unwrap_or(after.len());
        &after[..end]
    };
    let trimmed = value.trim();
    let lower_value = trimmed.to_ascii_lowercase();
    if lower_value.starts_with("http://")
        || lower_value.starts_with("https://")
        || lower_value.starts_with("mailto:")
    {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn escape_html_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
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
        assert_eq!(
            extract_domain("@user@example.com"),
            Some("example.com".to_string())
        );
        assert_eq!(
            extract_domain("user@example.com"),
            Some("example.com".to_string())
        );
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

    #[test]
    fn sanitize_html_removes_dangerous_tags_and_event_handlers() {
        let html = r#"<p onclick="steal()">Hello <script>alert(1)</script><strong>world</strong><img src=x onerror=bad()></p>"#;
        let sanitized = sanitize_html(html);
        assert_eq!(sanitized, "<p>Hello <strong>world</strong></p>");
    }

    #[test]
    fn sanitize_html_keeps_safe_links_and_drops_javascript_urls() {
        let html = r#"<a href="https://example.com/path?a=1&b=2" rel="me">safe</a><a href="javascript:alert(1)">bad</a>"#;
        let sanitized = sanitize_html(html);
        assert_eq!(
            sanitized,
            "<a href=\"https://example.com/path?a=1&amp;b=2\">safe</a>bad"
        );
    }

    #[test]
    fn sanitize_html_drops_style_blocks_and_unsafe_iframes() {
        let html = "<style>body{display:none}</style><iframe src=\"https://example.com\"></iframe><em>ok</em>";
        assert_eq!(sanitize_html(html), "<em>ok</em>");
    }
}
