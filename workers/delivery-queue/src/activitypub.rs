use worker::*;

/// Deliver an ActivityPub activity to a remote inbox with HTTP signatures
///
/// Note: This is a simplified version. Full HTTP signature implementation
/// should use the delivery logic from the CLI (dais_cli/delivery.py) which
/// has proper RSA-SHA256 signing with the actor's private key.
pub async fn deliver_activity(
    inbox_url: &str,
    actor_url: &str,
    activity_json: &str,
    env: &Env,
) -> Result<()> {
    // Parse the inbox URL to get host and path
    let url = Url::parse(inbox_url)
        .map_err(|e| Error::RustError(format!("Invalid inbox URL: {}", e)))?;

    let host = url.host_str()
        .ok_or_else(|| Error::RustError("No host in inbox URL".to_string()))?;
    let path = url.path();

    // Get private key from secrets
    let private_key_pem = env.secret("ACTOR_PRIVATE_KEY")
        .map_err(|_| Error::RustError("ACTOR_PRIVATE_KEY secret not found".to_string()))?
        .to_string();

    // Generate HTTP signature
    let date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();

    // For now, use a simple digest. In production, implement proper SHA-256 digest
    let digest = format!("SHA-256=placeholder");

    // Build signature string
    let string_to_sign = format!(
        "(request-target): post {}\nhost: {}\ndate: {}\ndigest: {}",
        path, host, date, digest
    );

    // For now, use placeholder signature
    // TODO: Implement proper RSA-SHA256 signing
    console_log!("Warning: Using placeholder HTTP signature");
    let signature = "placeholder_signature";

    // Extract key ID from actor URL
    let key_id = format!("{}#main-key", actor_url);

    let signature_header = format!(
        r#"keyId="{}",headers="(request-target) host date digest",signature="{}""#,
        key_id, signature
    );

    // Make the HTTP POST request
    let mut headers = Headers::new();
    headers.set("Host", host)?;
    headers.set("Date", &date)?;
    headers.set("Digest", &digest)?;
    headers.set("Signature", &signature_header)?;
    headers.set("Content-Type", "application/activity+json")?;
    headers.set("User-Agent", "dais/0.1.0")?;

    let mut request_init = RequestInit::new();
    request_init
        .with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(activity_json.into()));

    let request = Request::new_with_init(inbox_url, &request_init)?;
    let mut response = Fetch::Request(request).send().await?;

    if response.status_code() >= 200 && response.status_code() < 300 {
        console_log!("✓ Delivered to {}: {}", inbox_url, response.status_code());
        Ok(())
    } else {
        let error_body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        Err(Error::RustError(format!(
            "HTTP {} from {}: {}",
            response.status_code(),
            inbox_url,
            error_body
        )))
    }
}
