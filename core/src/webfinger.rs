use crate::traits::DatabaseProvider;
/// WebFinger protocol implementation
///
/// WebFinger is used to discover information about users across different domains.
/// See: RFC 7033
use crate::{CoreError, CoreResult};
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
        .execute(
            "SELECT username FROM actors WHERE username = ?1",
            &[serde_json::Value::String(username.to_string())],
        )
        .await?;

    if rows.is_empty() {
        return Err(CoreError::NotFound("User not found".to_string()));
    }

    // Build WebFinger response.
    // Always advertise the canonical apex handle as the subject (acct:user@base),
    // regardless of whether the apex or the AP-subdomain form was queried. This is
    // what makes @user@domain.com the canonical handle: when a remote server fetches
    // the actor (on the subdomain) and re-checks the canonical acct, it must get back
    // the apex handle, or it treats the apex as a mismatch and rejects it.
    let response = WebFingerResponse {
        subject: format!("acct:{}@{}", username, configured_domain),
        aliases: vec![format!("https://{}/users/{}", activitypub_domain, username)],
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
    use crate::traits::{DatabaseDialect, PlatformResult, Row, Statement};
    use async_trait::async_trait;
    use serde_json::Value;

    struct WebfingerDb;

    #[async_trait(?Send)]
    impl DatabaseProvider for WebfingerDb {
        async fn execute(&self, _sql: &str, params: &[Value]) -> PlatformResult<Vec<Row>> {
            if params.first().and_then(Value::as_str) == Some("social") {
                let mut row = Row::new();
                row.insert("username".to_string(), Value::String("social".to_string()));
                Ok(vec![row])
            } else {
                Ok(Vec::new())
            }
        }

        async fn batch(&self, _statements: Vec<Statement>) -> PlatformResult<()> {
            Ok(())
        }

        fn dialect(&self) -> DatabaseDialect {
            DatabaseDialect::SQLite
        }
    }

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

    #[tokio::test]
    async fn apex_handle_is_canonical_and_subdomain_acct_is_not_advertised() {
        let response = handle_webfinger(
            &WebfingerDb,
            "acct:social@dais.social",
            "dais.social",
            "social.dais.social",
        )
        .await
        .unwrap();
        assert_eq!(response.subject, "acct:social@dais.social");
        assert!(response
            .aliases
            .contains(&"https://social.dais.social/users/social".to_string()));
        assert!(!response
            .aliases
            .contains(&"acct:social@social.dais.social".to_string()));
    }
}
