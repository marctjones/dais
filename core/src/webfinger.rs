/// WebFinger protocol implementation
///
/// WebFinger is used to discover information about users across different domains.
/// See: RFC 7033

use crate::{CoreResult, CoreError};
use crate::traits::DatabaseProvider;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct WebFingerResponse {
    pub subject: String,
    pub aliases: Vec<String>,
    pub links: Vec<WebFingerLink>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebFingerLink {
    pub rel: String,
    #[serde(rename = "type")]
    pub link_type: String,
    pub href: String,
}

/// Handle a WebFinger request
///
/// # Arguments
/// * `db` - Database provider to check if user exists
/// * `resource` - Resource identifier (e.g., "acct:user@domain.com")
/// * `configured_domain` - Base domain (e.g., "dais.social")
/// * `activitypub_domain` - ActivityPub domain (e.g., "social.dais.social")
///
/// # Returns
/// WebFingerResponse with user information if found
pub async fn handle_webfinger(
    db: &dyn DatabaseProvider,
    resource: &str,
    configured_domain: &str,
    activitypub_domain: &str,
) -> CoreResult<WebFingerResponse> {
    // Parse the resource identifier
    // Expected format: acct:user@domain
    if !resource.starts_with("acct:") {
        return Err(CoreError::InvalidActivity(
            "Invalid resource format. Expected acct:user@domain".to_string(),
        ));
    }

    let account = resource.strip_prefix("acct:").unwrap();
    let parts: Vec<&str> = account.split('@').collect();

    if parts.len() != 2 {
        return Err(CoreError::InvalidActivity(
            "Invalid account format. Expected user@domain".to_string(),
        ));
    }

    let username = parts[0];
    let domain = parts[1];

    // Validate domain matches either our base domain or ActivityPub subdomain
    if domain != configured_domain && domain != activitypub_domain {
        return Err(CoreError::NotFound("Domain not found".to_string()));
    }

    // Query database to verify user exists
    let rows = db
        .execute("SELECT username FROM actors WHERE username = ?1", &[serde_json::Value::String(username.to_string())])
        .await?;

    if rows.is_empty() {
        return Err(CoreError::NotFound("User not found".to_string()));
    }

    // Build WebFinger response
    let response = WebFingerResponse {
        subject: resource.to_string(),
        aliases: vec![
            format!("https://{}/users/{}", activitypub_domain, username),
        ],
        links: vec![
            WebFingerLink {
                rel: "self".to_string(),
                link_type: "application/activity+json".to_string(),
                href: format!("https://{}/users/{}", activitypub_domain, username),
            },
            WebFingerLink {
                rel: "http://webfinger.net/rel/profile-page".to_string(),
                link_type: "text/html".to_string(),
                href: format!("https://{}/@{}", configured_domain, username),
            },
        ],
    };

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_resource() {
        let resource = "acct:user@example.com";
        assert!(resource.starts_with("acct:"));

        let account = resource.strip_prefix("acct:").unwrap();
        let parts: Vec<&str> = account.split('@').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "user");
        assert_eq!(parts[1], "example.com");
    }
}
