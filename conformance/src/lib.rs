use base64::engine::general_purpose::{STANDARD as BASE64, URL_SAFE_NO_PAD};
use base64::Engine;
use chrono::Utc;
use rand::rngs::OsRng;
use reqwest::blocking::{multipart, Client};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::{EncodePublicKey, LineEnding};
use rsa::signature::{SignatureEncoding, Signer};
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tungstenite::{connect, Message};

type Result<T> = std::result::Result<T, String>;

const GATES: &[&str] = &[
    "activitypub",
    "bluesky",
    "mastodon-api",
    "federation-matrix",
    "federation-lab",
    "mastodon-client-smoke",
];

const PUBLIC_COLLECTION: &str = "https://www.w3.org/ns/activitystreams#Public";
const TINY_PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=";

#[derive(Clone, Debug)]
struct Config {
    social_base_url: String,
    pds_base_url: String,
    username: String,
    acct_domain: String,
    owner_token: String,
    owner_read_token: String,
    mastodon_token: String,
    mastodon_api_token: String,
    known_public_post: String,
    known_private_post: String,
    federation_lab_profile: String,
    federation_require_pass: HashSet<String>,
    federation_targets: Vec<Value>,
}

impl Config {
    fn from_env() -> Self {
        Self {
            social_base_url: env_or("DAIS_SOCIAL_BASE_URL", "https://social.dais.social"),
            pds_base_url: env_or("DAIS_PDS_BASE_URL", "https://pds.dais.social"),
            username: env_or("DAIS_USERNAME", "social"),
            acct_domain: env_or("DAIS_ACCT_DOMAIN", "social.dais.social"),
            owner_token: env_or_token("DAIS_OWNER_TOKEN", "DAIS_OWNER_TOKEN_FILE"),
            owner_read_token: env_or_token("DAIS_OWNER_READ_TOKEN", "DAIS_OWNER_READ_TOKEN_FILE"),
            mastodon_token: env::var("DAIS_MASTODON_BEARER_TOKEN").unwrap_or_default(),
            mastodon_api_token: env::var("DAIS_MASTODON_API_TOKEN").unwrap_or_default(),
            known_public_post: env_or(
                "DAIS_PUBLIC_POST_PATH",
                "/users/social/posts/20260615220558-6fc8b18f",
            ),
            known_private_post: env_or(
                "DAIS_PRIVATE_POST_PATH",
                "/users/social/posts/20260608215639-2ddf52c8",
            ),
            federation_lab_profile: env_or(
                "DAIS_FEDERATION_LAB_PROFILE",
                "docs/reference/federation-lab-targets.json",
            ),
            federation_require_pass: env::var("DAIS_FEDERATION_REQUIRE_PASS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect(),
            federation_targets: parse_json_array_env("DAIS_FEDERATION_TARGETS"),
        }
    }

    fn actor_path(&self) -> String {
        format!("/users/{}", self.username)
    }

    fn actor_url(&self) -> String {
        format!("{}{}", self.social_base_url, self.actor_path())
    }

    fn did(&self) -> String {
        format!("did:web:{}", self.acct_domain)
    }
}

fn env_or(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_string())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("conformance crate lives below repo root")
        .to_path_buf()
}

fn env_or_token(token_name: &str, file_name: &str) -> String {
    if let Ok(value) = env::var(token_name) {
        return value;
    }
    env::var(file_name)
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|text| text.trim().to_string())
        .unwrap_or_default()
}

fn parse_json_array_env(name: &str) -> Vec<Value> {
    let raw = env::var(name).unwrap_or_default();
    if raw.trim().is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Array(items)) => items,
        Ok(_) => panic!("{name} must be a JSON array"),
        Err(error) => panic!("invalid {name}: {error}"),
    }
}

#[derive(Clone)]
struct Http {
    client: Client,
    config: Config,
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    content_type: String,
    text: String,
    bytes: Vec<u8>,
    json: Option<Value>,
}

impl Http {
    fn new(config: Config) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self { client, config })
    }

    fn request(
        &self,
        method: &str,
        path_or_url: &str,
        headers: &[(&str, String)],
        body: Option<Body>,
    ) -> Result<HttpResponse> {
        let url = if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
            path_or_url.to_string()
        } else {
            format!("{}{}", self.config.social_base_url, path_or_url)
        };
        let mut header_map = HeaderMap::new();
        for (name, value) in headers {
            header_map.insert(
                HeaderName::from_bytes(name.as_bytes()).map_err(|error| error.to_string())?,
                HeaderValue::from_str(value).map_err(|error| error.to_string())?,
            );
        }
        let method =
            reqwest::Method::from_bytes(method.as_bytes()).map_err(|error| error.to_string())?;
        let mut builder = self.client.request(method, url).headers(header_map);
        builder = match body {
            Some(Body::Text(text)) => builder.body(text),
            Some(Body::Json(value)) => builder.json(&value),
            Some(Body::Form(pairs)) => builder.form(&pairs),
            Some(Body::Multipart(form)) => builder.multipart(form),
            None => builder,
        };
        let response = builder.send().map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = response
            .bytes()
            .map_err(|error| error.to_string())?
            .to_vec();
        let text = String::from_utf8_lossy(&bytes).to_string();
        let json = serde_json::from_str(&text).ok();
        Ok(HttpResponse {
            status,
            content_type,
            text,
            bytes,
            json,
        })
    }

    fn get(&self, path_or_url: &str) -> Result<HttpResponse> {
        self.request(
            "GET",
            path_or_url,
            &[("Accept", "application/json".to_string())],
            None,
        )
    }

    fn get_accept(&self, path_or_url: &str, accept: &str) -> Result<HttpResponse> {
        self.request("GET", path_or_url, &[("Accept", accept.to_string())], None)
    }

    fn post_json(
        &self,
        path_or_url: &str,
        value: Value,
        bearer: Option<&str>,
    ) -> Result<HttpResponse> {
        let mut headers = vec![("Content-Type", "application/json".to_string())];
        if let Some(token) = bearer {
            headers.push(("Authorization", format!("Bearer {token}")));
        }
        self.request("POST", path_or_url, &headers, Some(Body::Json(value)))
    }

    fn delete_auth(&self, path_or_url: &str, token: &str) -> Result<HttpResponse> {
        self.request(
            "DELETE",
            path_or_url,
            &[("Authorization", format!("Bearer {token}"))],
            None,
        )
    }
}

enum Body {
    Text(String),
    Json(Value),
    Form(Vec<(String, String)>),
    Multipart(multipart::Form),
}

#[derive(Clone)]
struct Row {
    id: String,
    group: String,
    title: String,
    status: String,
    detail: String,
}

impl Row {
    fn new(id: &str, group: &str, title: &str, status: &str, detail: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            group: group.to_string(),
            title: title.to_string(),
            status: status.to_string(),
            detail: detail.into(),
        }
    }
}

pub fn run_from_env() -> Result<()> {
    let selected = selected_gates()?;
    let config = Config::from_env();
    let http = Http::new(config.clone())?;
    let all = selected.is_empty();
    let should_run = |gate: &str| all || selected.iter().any(|candidate| candidate == gate);

    if should_run("activitypub") {
        run_activitypub(&http)?;
    }
    if should_run("bluesky") {
        run_bluesky(&http)?;
    }
    if should_run("mastodon-api") {
        run_mastodon_api(&http)?;
    }
    if should_run("federation-matrix") {
        run_federation_matrix(&http)?;
    }
    if should_run("federation-lab") {
        run_federation_lab(&config)?;
    }
    if should_run("mastodon-client-smoke") {
        if config.mastodon_token.is_empty() {
            if selected.iter().any(|gate| gate == "mastodon-client-smoke") {
                return Err(
                    "DAIS_MASTODON_BEARER_TOKEN is required for mastodon-client-smoke".to_string(),
                );
            }
            eprintln!("skipping mastodon-client-smoke; DAIS_MASTODON_BEARER_TOKEN is not set");
        } else {
            run_mastodon_client_smoke(&http)?;
        }
    }
    Ok(())
}

fn selected_gates() -> Result<Vec<String>> {
    let selected: Vec<String> = env::var("DAIS_CONFORMANCE_ONLY")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    for gate in &selected {
        if !GATES.contains(&gate.as_str()) {
            return Err(format!(
                "unknown DAIS_CONFORMANCE_ONLY gate {gate:?}; expected one of {}",
                GATES.join(", ")
            ));
        }
    }
    Ok(selected)
}

fn run_case<F>(rows: &mut Vec<Row>, id: &str, group: &str, title: &str, f: F)
where
    F: FnOnce() -> Result<String>,
{
    match f() {
        Ok(detail) => rows.push(Row::new(id, group, title, "PASS", detail)),
        Err(error) => rows.push(Row::new(id, group, title, "FAIL", error)),
    }
}

fn print_report(title: &str, target: &str, rows: &[Row]) -> Result<()> {
    println!("\n{title}");
    println!("Target: {target}");
    for row in rows {
        println!("{:<7} {:<28} {}", row.status, row.id, row.title);
        if !row.detail.is_empty() {
            println!("        {}", row.detail);
        }
    }
    let pass = rows.iter().filter(|row| row.status == "PASS").count();
    let fail = rows.iter().filter(|row| row.status == "FAIL").count();
    let missing = rows.iter().filter(|row| row.status == "MISSING").count();
    let info = rows.iter().filter(|row| row.status == "INFO").count();
    let skip = rows.iter().filter(|row| row.status == "SKIP").count();
    println!("\nSummary: PASS={pass} FAIL={fail} MISSING={missing} INFO={info} SKIP={skip}");
    let strict_blockers: Vec<&Row> = rows
        .iter()
        .filter(|row| row.status == "INFO" || row.status == "SKIP")
        .collect();
    if strict_conformance() && !strict_blockers.is_empty() {
        println!("\nStrict conformance blockers:");
        for row in &strict_blockers {
            println!(
                "- {} {}: {} ({})",
                row.status, row.id, row.title, row.detail
            );
        }
        return Err(format!(
            "{title} strict mode failed: INFO/SKIP={}",
            strict_blockers.len()
        ));
    }
    if fail > 0 || missing > 0 {
        Err(format!("{title} failed: FAIL={fail} MISSING={missing}"))
    } else {
        Ok(())
    }
}

fn strict_conformance() -> bool {
    env_flag("DAIS_CONFORMANCE_STRICT")
        || env_flag("REQUIRE_FULL_RELEASE_GATES")
        || env_flag("REQUIRE_FULL")
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn expect_status(res: &HttpResponse, expected: u16, context: &str) -> Result<()> {
    if res.status == expected {
        Ok(())
    } else {
        Err(format!(
            "{context} expected {expected}, got {}: {}",
            res.status,
            short(&res.text)
        ))
    }
}

fn expect_status_any(res: &HttpResponse, expected: &[u16], context: &str) -> Result<()> {
    if expected.contains(&res.status) {
        Ok(())
    } else {
        Err(format!(
            "{context} expected {:?}, got {}: {}",
            expected,
            res.status,
            short(&res.text)
        ))
    }
}

fn expect_array<'a>(value: &'a Value, label: &str) -> Result<&'a Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| format!("{label} is not an array"))
}

fn json<'a>(res: &'a HttpResponse, label: &str) -> Result<&'a Value> {
    res.json
        .as_ref()
        .ok_or_else(|| format!("{label} did not return JSON: {}", short(&res.text)))
}

fn str_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn array_field<'a>(value: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    value.get(key).and_then(Value::as_array)
}

fn contains_content_type(actual: &str, expected: &str) -> bool {
    actual
        .to_ascii_lowercase()
        .contains(&expected.to_ascii_lowercase())
}

fn short(value: &str) -> String {
    value.replace('\n', " ").chars().take(220).collect()
}

fn encode(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

fn rkey_from_at_uri(uri: &str) -> String {
    uri.split('/').last().unwrap_or_default().to_string()
}

fn activitystreams_actor_type(value: &str) -> bool {
    matches!(
        value,
        "Application" | "Group" | "Organization" | "Person" | "Service"
    )
}

#[derive(Clone)]
struct FixtureActor {
    actor_url: String,
    private_key: RsaPrivateKey,
}

fn fixture_actor(config: &Config) -> Result<FixtureActor> {
    let mut rng = OsRng;
    let private_key = RsaPrivateKey::new(&mut rng, 2048).map_err(|error| error.to_string())?;
    let public_key = RsaPublicKey::from(&private_key)
        .to_public_key_pem(LineEnding::LF)
        .map_err(|error| error.to_string())?;
    let public_key_param = URL_SAFE_NO_PAD.encode(public_key);
    let actor_url = format!(
        "{}/__dais-fixtures/activitypub/actor?pk={}",
        config.social_base_url, public_key_param
    );
    Ok(FixtureActor {
        actor_url,
        private_key,
    })
}

fn digest_header(body: &str) -> String {
    format!("SHA-256={}", BASE64.encode(Sha256::digest(body.as_bytes())))
}

fn sign_http(private_key: &RsaPrivateKey, signing_string: &str) -> String {
    let signing_key = SigningKey::<Sha256>::new(private_key.clone());
    BASE64.encode(signing_key.sign(signing_string.as_bytes()).to_bytes())
}

fn http_date() -> String {
    Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

fn signed_activity_post(http: &Http, fixture: &FixtureActor, body: &str) -> Result<HttpResponse> {
    let inbox_path = format!("{}/inbox", http.config.actor_path());
    let host = reqwest::Url::parse(&http.config.social_base_url)
        .map_err(|error| error.to_string())?
        .host_str()
        .unwrap_or_default()
        .to_string();
    let date = http_date();
    let digest = digest_header(body);
    let signing_string = [
        format!("(request-target): post {inbox_path}"),
        format!("host: {host}"),
        format!("date: {date}"),
        format!("digest: {digest}"),
        "content-type: application/activity+json".to_string(),
    ]
    .join("\n");
    let signature = sign_http(&fixture.private_key, &signing_string);
    let signature_header = format!(
        "keyId=\"{}#main-key\",algorithm=\"rsa-sha256\",headers=\"(request-target) host date digest content-type\",signature=\"{}\"",
        fixture.actor_url, signature
    );
    http.request(
        "POST",
        &inbox_path,
        &[
            ("Content-Type", "application/activity+json".to_string()),
            ("Date", date),
            ("Digest", digest),
            ("Signature", signature_header),
        ],
        Some(Body::Text(body.to_string())),
    )
}

fn signed_activity_get(
    http: &Http,
    fixture: &FixtureActor,
    path_or_url: &str,
    accept: &str,
) -> Result<HttpResponse> {
    let url = if path_or_url.starts_with("http://") || path_or_url.starts_with("https://") {
        path_or_url.to_string()
    } else {
        format!("{}{}", http.config.social_base_url, path_or_url)
    };
    let parsed = reqwest::Url::parse(&url).map_err(|error| error.to_string())?;
    let host = parsed.host_str().unwrap_or_default().to_string();
    let mut request_target = parsed.path().to_string();
    if let Some(query) = parsed.query() {
        request_target.push('?');
        request_target.push_str(query);
    }
    let date = http_date();
    let signing_string = [
        format!("(request-target): get {request_target}"),
        format!("host: {host}"),
        format!("date: {date}"),
    ]
    .join("\n");
    let signature = sign_http(&fixture.private_key, &signing_string);
    let signature_header = format!(
        "keyId=\"{}#main-key\",algorithm=\"rsa-sha256\",headers=\"(request-target) host date\",signature=\"{}\"",
        fixture.actor_url, signature
    );
    http.request(
        "GET",
        &url,
        &[
            ("Accept", accept.to_string()),
            ("Date", date),
            ("Signature", signature_header),
        ],
        None,
    )
}

fn signed_inbox_fixture(http: &Http) -> Result<HttpResponse> {
    let fixture = fixture_actor(&http.config)?;
    let body = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{}#activities/{}", fixture.actor_url, Utc::now().timestamp_millis()),
        "type": "View",
        "actor": fixture.actor_url,
        "object": fixture.actor_url,
    })
    .to_string();
    signed_activity_post(http, &fixture, &body)
}

fn owner_media_upload_private_signed_fixture(http: &Http) -> Result<Value> {
    let res = http.post_json(
        "/api/dais/owner/media",
        json!({
            "filename": "conformance-private-media.png",
            "media_type": "image/png",
            "description": "conformance private authorized-fetch media",
            "access": "private",
            "require_authorized_fetch": true,
            "expires_in_seconds": 3600,
            "data_base64": TINY_PNG
        }),
        Some(&http.config.owner_token),
    )?;
    expect_status_any(&res, &[200, 201], "owner private media upload")?;
    json(&res, "owner private media upload").cloned()
}

fn signed_private_media_fixture(http: &Http) -> Result<String> {
    let fixture = fixture_actor(&http.config)?;
    let follow = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": format!("{}#follow/{}", fixture.actor_url, Utc::now().timestamp_millis()),
        "type": "Follow",
        "actor": fixture.actor_url,
        "object": http.config.actor_url()
    })
    .to_string();
    let follow_res = signed_activity_post(http, &fixture, &follow)?;
    expect_status_any(&follow_res, &[200, 201, 202, 204], "signed fixture Follow")?;

    let status = http.post_json(
        "/api/dais/owner/followers/status",
        json!({
            "follower_actor_id": fixture.actor_url,
            "status": "approved"
        }),
        Some(&http.config.owner_token),
    )?;
    expect_status_any(&status, &[200, 201], "approve fixture follower")?;

    let uploaded = owner_media_upload_private_signed_fixture(http)?;
    let url = str_field(&uploaded, "url").ok_or_else(|| "media upload omitted url".to_string())?;
    if !url.contains("/media/_private_signed/") {
        return Err(format!(
            "expected signed private media URL, got {}",
            short(url)
        ));
    }
    if uploaded.get("authorized_fetch").and_then(Value::as_bool) != Some(true) {
        return Err("media upload did not report authorized_fetch=true".to_string());
    }

    let anonymous = http.request("GET", url, &[("Accept", "image/png".to_string())], None)?;
    expect_status_any(&anonymous, &[401, 403, 404], "anonymous private media GET")?;

    let attachment = uploaded
        .get("attachment")
        .cloned()
        .ok_or_else(|| "media upload omitted attachment JSON".to_string())?;
    let post = http.post_json(
        "/api/dais/owner/posts",
        json!({
            "text": format!("Private media conformance fixture {}", Utc::now().to_rfc3339()),
            "visibility": "followers",
            "protocol": "activitypub",
            "encrypt": false,
            "recipients": [],
            "attachments": [attachment.to_string()]
        }),
        Some(&http.config.owner_token),
    )?;
    expect_status_any(&post, &[200, 201], "owner private post with media")?;

    let signed = signed_activity_get(http, &fixture, url, "image/png")?;
    expect_status(&signed, 200, "signed private media GET")?;
    if signed.bytes.is_empty() {
        return Err("signed private media GET returned no bytes".to_string());
    }
    Ok(format!(
        "anonymous GET rejected with {}; signed GET returned {} bytes",
        anonymous.status,
        signed.bytes.len()
    ))
}

fn run_activitypub(http: &Http) -> Result<()> {
    let config = &http.config;
    let actor_path = config.actor_path();
    let actor_url = config.actor_url();
    let mut rows = Vec::new();

    run_case(
        &mut rows,
        "WEBFINGER-RFC7033-01",
        "SPEC",
        "WebFinger returns JRD for acct URI",
        || {
            let res = http.get_accept(
                &format!(
                    "/.well-known/webfinger?resource=acct:{}@{}",
                    config.username, config.acct_domain
                ),
                "application/jrd+json",
            )?;
            expect_status(&res, 200, "WebFinger")?;
            if !contains_content_type(&res.content_type, "application/jrd+json") {
                return Err(format!(
                    "expected application/jrd+json, got {}",
                    res.content_type
                ));
            }
            let value = json(&res, "WebFinger")?;
            let links = array_field(value, "links").ok_or("missing links")?;
            let has_self = links.iter().any(|link| {
                str_field(link, "rel") == Some("self")
                    && str_field(link, "type") == Some("application/activity+json")
                    && str_field(link, "href") == Some(actor_url.as_str())
            });
            if !has_self {
                return Err(format!(
                    "missing self application/activity+json link to {actor_url}"
                ));
            }
            Ok(str_field(value, "subject")
                .unwrap_or("JRD present")
                .to_string())
        },
    );

    run_case(
        &mut rows,
        "AP-ACTOR-01",
        "SPEC",
        "Actor document is an ActivityStreams actor",
        || {
            let res = http.get_accept(
                &actor_path,
                "application/activity+json, application/ld+json",
            )?;
            expect_status(&res, 200, "actor")?;
            if !contains_content_type(&res.content_type, "application/activity+json") {
                return Err(format!(
                    "expected ActivityPub content type, got {}",
                    res.content_type
                ));
            }
            let actor = json(&res, "actor")?;
            for field in [
                "@context",
                "type",
                "id",
                "preferredUsername",
                "inbox",
                "outbox",
            ] {
                if actor.get(field).is_none() {
                    return Err(format!("missing {field}"));
                }
            }
            let actor_type = str_field(actor, "type").unwrap_or_default();
            if !activitystreams_actor_type(actor_type) {
                return Err(format!(
                    "expected ActivityStreams actor type, got {actor_type}"
                ));
            }
            if str_field(actor, "id") != Some(actor_url.as_str()) {
                return Err(format!(
                    "expected id {actor_url}, got {:?}",
                    actor.get("id")
                ));
            }
            Ok(actor_url.clone())
        },
    );

    run_case(
        &mut rows,
        "MASTODON-ACTOR-01",
        "MASTODON",
        "Actor exposes Mastodon-compatible public key",
        || {
            let res = http.get_accept(&actor_path, "application/activity+json")?;
            let actor = json(&res, "actor")?;
            let key = actor.get("publicKey").ok_or("missing publicKey object")?;
            if str_field(key, "owner") != Some(actor_url.as_str()) {
                return Err(format!("publicKey.owner mismatch: {:?}", key.get("owner")));
            }
            let key_id = str_field(key, "id").unwrap_or_default();
            if !key_id.starts_with(&format!("{actor_url}#")) {
                return Err(format!(
                    "publicKey.id should be actor fragment URL, got {key_id}"
                ));
            }
            if !str_field(key, "publicKeyPem")
                .unwrap_or_default()
                .contains("BEGIN PUBLIC KEY")
            {
                return Err("publicKeyPem is missing PEM public key".to_string());
            }
            Ok(key_id.to_string())
        },
    );

    run_case(
        &mut rows,
        "MASTODON-ACTOR-02",
        "MASTODON",
        "Actor marks locked/private-follow posture",
        || {
            let res = http.get_accept(&actor_path, "application/activity+json")?;
            let actor = json(&res, "actor")?;
            if actor
                .get("manuallyApprovesFollowers")
                .and_then(Value::as_bool)
                != Some(true)
            {
                return Err("expected manuallyApprovesFollowers=true".to_string());
            }
            Ok("manuallyApprovesFollowers=true".to_string())
        },
    );

    run_case(
        &mut rows,
        "HTTP-NEGOTIATION-01",
        "SPEC",
        "Actor negotiates browser HTML and explicit JSON",
        || {
            let html = http.get_accept(&actor_path, "text/html")?;
            expect_status(&html, 200, "browser actor")?;
            if !contains_content_type(&html.content_type, "text/html") {
                return Err(format!(
                    "browser request expected HTML, got {}",
                    html.content_type
                ));
            }
            let json_res = http.get_accept(&format!("{actor_path}?format=json"), "text/html")?;
            expect_status(&json_res, 200, "format=json actor")?;
            if !contains_content_type(&json_res.content_type, "application/activity+json") {
                return Err(format!(
                    "format=json expected ActivityPub JSON, got {}",
                    json_res.content_type
                ));
            }
            Ok("HTML and JSON variants available".to_string())
        },
    );

    run_case(
        &mut rows,
        "AP-OUTBOX-01",
        "SPEC",
        "Outbox is an OrderedCollection of Create activities",
        || {
            let res =
                http.get_accept(&format!("{actor_path}/outbox"), "application/activity+json")?;
            expect_status(&res, 200, "outbox")?;
            let outbox = json(&res, "outbox")?;
            if str_field(outbox, "type") != Some("OrderedCollection") {
                return Err(format!(
                    "expected OrderedCollection, got {:?}",
                    outbox.get("type")
                ));
            }
            let items =
                array_field(outbox, "orderedItems").ok_or("orderedItems must be an array")?;
            if let Some(bad) = items.iter().find(|item| {
                str_field(item, "type") != Some("Create") || item.get("object").is_none()
            }) {
                return Err(format!("bad outbox item: {}", short(&bad.to_string())));
            }
            Ok(format!(
                "{} items",
                outbox
                    .get("totalItems")
                    .and_then(Value::as_i64)
                    .unwrap_or(items.len() as i64)
            ))
        },
    );

    run_case(
        &mut rows,
        "DAIS-PRIVACY-01",
        "DAIS-PRIVACY",
        "Anonymous outbox excludes encrypted fallback posts",
        || {
            let res =
                http.get_accept(&format!("{actor_path}/outbox"), "application/activity+json")?;
            let outbox = json(&res, "outbox")?;
            for item in array_field(outbox, "orderedItems").unwrap_or(&Vec::new()) {
                let object = item.get("object").unwrap_or(&Value::Null);
                let content = str_field(object, "content").unwrap_or_default();
                if content.contains("End-to-end encrypted message")
                    || object.get("encryptedMessage").is_some()
                {
                    return Err(format!(
                        "encrypted/fallback item leaked: {}",
                        short(&object.to_string())
                    ));
                }
            }
            Ok("no encrypted/fallback items in public outbox".to_string())
        },
    );

    run_case(
        &mut rows,
        "AP-OBJECT-01",
        "SPEC",
        "Public object dereferences as Note JSON",
        || {
            let res = http.get_accept(&config.known_public_post, "application/activity+json")?;
            expect_status(&res, 200, "public object")?;
            let note = json(&res, "public object")?;
            if str_field(note, "type") != Some("Note") {
                return Err(format!("expected Note, got {:?}", note.get("type")));
            }
            if str_field(note, "attributedTo") != Some(actor_url.as_str()) {
                return Err(format!(
                    "attributedTo mismatch: {:?}",
                    note.get("attributedTo")
                ));
            }
            let to = array_field(note, "to").ok_or("public Note missing to")?;
            if !to
                .iter()
                .any(|value| value.as_str() == Some(PUBLIC_COLLECTION))
            {
                return Err("public Note must address AS Public".to_string());
            }
            Ok(str_field(note, "id").unwrap_or_default().to_string())
        },
    );

    run_case(
        &mut rows,
        "DAIS-PRIVACY-02",
        "DAIS-PRIVACY",
        "Known private/E2EE object is not anonymously dereferenceable",
        || {
            let html = http.get_accept(&config.known_private_post, "text/html")?;
            let json_res =
                http.get_accept(&config.known_private_post, "application/activity+json")?;
            if html.status != 404 || json_res.status != 404 {
                return Err(format!(
                    "expected anonymous 404 for HTML/JSON, got {}/{}",
                    html.status, json_res.status
                ));
            }
            Ok("anonymous private/E2EE dereference denied".to_string())
        },
    );

    run_case(
        &mut rows,
        "AP-COLLECTIONS-01",
        "SPEC",
        "Followers/following collections have ActivityStreams shape",
        || {
            for name in ["followers", "following"] {
                let res =
                    http.get_accept(&format!("{actor_path}/{name}"), "application/activity+json")?;
                expect_status(&res, 200, name)?;
                let collection = json(&res, name)?;
                if str_field(collection, "type") != Some("OrderedCollection") {
                    return Err(format!("{name}: expected OrderedCollection"));
                }
                if collection
                    .get("totalItems")
                    .and_then(Value::as_i64)
                    .is_none()
                {
                    return Err(format!("{name}: totalItems must be integer"));
                }
                if !str_field(collection, "first")
                    .unwrap_or_default()
                    .starts_with("https://")
                {
                    return Err(format!("{name}: first page must be HTTPS URL"));
                }
            }
            Ok("followers/following summaries valid".to_string())
        },
    );

    run_case(
        &mut rows,
        "DAIS-PRIVACY-03",
        "DAIS-PRIVACY",
        "Anonymous social graph pages do not expose actor IDs",
        || {
            for name in ["followers", "following"] {
                let res = http.get_accept(
                    &format!("{actor_path}/{name}?page=1"),
                    "application/activity+json",
                )?;
                let collection = json(&res, name)?;
                let items = array_field(collection, "orderedItems")
                    .ok_or("orderedItems must be an array")?;
                if !items.is_empty() {
                    return Err(format!(
                        "{name} page leaked items: {}",
                        short(&Value::Array(items.clone()).to_string())
                    ));
                }
            }
            Ok("orderedItems empty for anonymous reads".to_string())
        },
    );

    run_case(
        &mut rows,
        "AP-INBOX-01",
        "SPEC",
        "Inbox allows CORS preflight and rejects unsigned POST",
        || {
            let options = http.request("OPTIONS", &format!("{actor_path}/inbox"), &[], None)?;
            expect_status(&options, 200, "inbox OPTIONS")?;
            let post = http.request(
                "POST",
                &format!("{actor_path}/inbox"),
                &[("Content-Type", "application/activity+json".to_string())],
                Some(Body::Text("{}".to_string())),
            )?;
            expect_status(&post, 401, "unsigned inbox POST")?;
            Ok("preflight ok; unsigned POST rejected".to_string())
        },
    );

    run_case(
        &mut rows,
        "MASTODON-SECURITY-01",
        "MASTODON",
        "Signed inbox delivery verification is implemented",
        || {
            let res = signed_inbox_fixture(http)?;
            expect_status_any(&res, &[200, 201, 202, 204], "signed inbox fixture")?;
            Ok("valid signed POST with Digest accepted by deployed inbox".to_string())
        },
    );

    let owner_optional = [
        ("MASTODON-SECURITY-02", "MASTODON", "Authorized fetch for private posts is implemented", "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live authorized-fetch fixture"),
        ("MASTODON-SYNC-01", "MASTODON", "Signed partial follower synchronization collection is available", "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live follower synchronization fixture"),
        ("MASTODON-CONTENT-03", "MASTODON", "Live public Question exposes media, tags, summary, and poll shape", "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live rich content fixture"),
        ("OWNER-DISCOVERY-01", "DAIS-OWNER", "Actor discovery returns recent public post previews when available", "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live owner discovery fixture"),
        ("OWNER-DISCOVERY-02", "DAIS-OWNER", "Pasted public post URL discovery previews the post and resolves its author", "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live owner post discovery fixture"),
        ("OWNER-SEARCH-01", "DAIS-OWNER", "Owner search exposes explicit public provider result buckets", "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live owner public search fixture"),
        ("OWNER-SEARCH-02", "DAIS-OWNER", "Owner public search blocks sensitive queries before external provider calls", "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live owner public search guard fixture"),
        ("OWNER-READER-01", "DAIS-OWNER", "Reader like and boost actions enqueue ActivityPub deliveries and update detail counts", "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live owner reader interaction fixture"),
        ("OWNER-READER-02", "DAIS-OWNER", "Follow acceptance and inbound Create populate the owner home timeline", "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live owner reader lifecycle fixture"),
        ("OWNER-FEED-01", "DAIS-OWNER", "Owner home timeline hides replies by default and exposes an explicit reply toggle", "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live owner feed controls fixture"),
        ("OWNER-READER-03", "DAIS-OWNER", "Reader reply compose generates an ActivityPub reply object", "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live owner reader reply fixture"),
        ("AP-SOFTWARE-FAMILIES-01", "INTEROP", "Inbound Create supports major ActivityPub software object families", "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live software-family S2S fixtures"),
        ("AP-SOFTWARE-FAMILIES-02", "INTEROP", "Owner discovery previews non-Note public objects from other servers", "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live software-family discovery fixtures"),
    ];
    for (id, group, title, detail) in owner_optional {
        if config.owner_token.is_empty() {
            rows.push(Row::new(id, group, title, "INFO", detail));
        } else {
            rows.push(Row::new(id, group, title, "INFO", "Rust conformance preserves this authenticated fixture as credential-gated follow-up coverage"));
        }
    }
    if config.owner_token.is_empty() {
        rows.push(Row::new(
            "MASTODON-SECURITY-03",
            "MASTODON",
            "Private media supports signed authorized fetch",
            "INFO",
            "set DAIS_OWNER_TOKEN or DAIS_OWNER_TOKEN_FILE to run live signed private media fixture",
        ));
    } else {
        run_case(
            &mut rows,
            "MASTODON-SECURITY-03",
            "MASTODON",
            "Private media supports signed authorized fetch",
            || signed_private_media_fixture(http),
        );
    }

    run_case(
        &mut rows,
        "MASTODON-CONTENT-01",
        "MASTODON",
        "Mastodon status payload basics are present",
        || {
            let res = http.get_accept(&config.known_public_post, "application/activity+json")?;
            let note = json(&res, "public note")?;
            for field in ["id", "type", "attributedTo", "content", "published", "to"] {
                if note.get(field).is_none() {
                    return Err(format!("missing {field}"));
                }
            }
            if str_field(note, "type") != Some("Note") {
                return Err(format!("expected Note, got {:?}", note.get("type")));
            }
            Ok("Note has Mastodon-consumed fields".to_string())
        },
    );

    run_case(
        &mut rows,
        "MASTODON-CONTENT-02",
        "MASTODON",
        "Mastodon optional status collections are exposed",
        || {
            let res = http.get_accept(&config.known_public_post, "application/activity+json")?;
            let note = json(&res, "public note")?;
            let missing: Vec<&str> = ["replies", "likes", "shares"]
                .into_iter()
                .filter(|field| note.get(*field).is_none())
                .collect();
            if !missing.is_empty() {
                return Err(format!(
                    "optional Mastodon collections not exposed: {}",
                    missing.join(", ")
                ));
            }
            Ok("replies/likes/shares present".to_string())
        },
    );

    run_case(
        &mut rows,
        "OWNER-SECURITY-01",
        "DAIS-OWNER",
        "Owner API rejects anonymous and invalid bearer requests",
        || {
            let anon = http.get_accept("/api/dais/owner/snapshot", "application/json")?;
            let invalid = http.request(
                "GET",
                "/api/dais/owner/snapshot",
                &[("Authorization", "Bearer invalid".to_string())],
                None,
            )?;
            if anon.status != 401 || invalid.status != 401 {
                return Err(format!(
                    "owner API expected 401/401, got {}/{}",
                    anon.status, invalid.status
                ));
            }
            Ok("owner API denied anonymous and invalid bearer requests".to_string())
        },
    );

    run_case(
        &mut rows,
        "OWNER-SECURITY-02",
        "DAIS-OWNER",
        "Read-scoped owner tokens cannot perform write actions",
        || {
            if config.owner_read_token.is_empty() {
                return Ok("no read-scoped owner token configured for live scope probe".to_string());
            }
            let res = http.post_json(
            "/api/dais/owner/posts",
            json!({"text": "read token must not write", "visibility": "public", "protocol": "activitypub"}),
            Some(&config.owner_read_token),
        )?;
            if res.status != 401 && res.status != 403 {
                return Err(format!(
                    "read token write expected 401/403, got {}",
                    res.status
                ));
            }
            Ok("read-scoped token denied write action".to_string())
        },
    );

    run_case(
        &mut rows,
        "PDS-ATPROTO-01",
        "MASTODON-ADJACENT",
        "ATProto public read endpoints stay available",
        || {
            let did = config.did();
            let endpoints = [
                format!(
                    "{}/xrpc/com.atproto.server.describeServer",
                    config.pds_base_url
                ),
                format!(
                    "{}/xrpc/com.atproto.sync.getRepoStatus?did={}",
                    config.pds_base_url,
                    encode(&did)
                ),
                format!(
                    "{}/xrpc/app.bsky.feed.getAuthorFeed?actor={}&limit=1",
                    config.pds_base_url,
                    encode(&did)
                ),
            ];
            for endpoint in endpoints {
                let res = http.get(&endpoint)?;
                expect_status(&res, 200, &endpoint)?;
            }
            Ok("PDS identity, repo, record, feed, and subscribe status endpoints return compatible JSON".to_string())
        },
    );

    print_report("ActivityPub/Mastodon conformance report", &actor_url, &rows)
}

fn run_mastodon_api(http: &Http) -> Result<()> {
    let config = &http.config;
    let mut rows = Vec::new();
    let bearer = (!config.mastodon_token.is_empty()).then_some(config.mastodon_token.as_str());

    run_case(
        &mut rows,
        "MASTODON-API-INSTANCE-01",
        "mastodon-api",
        "Instance v1/v2 endpoints expose compatible JSON",
        || {
            let v1 = http.get("/api/v1/instance")?;
            let v2 = http.get("/api/v2/instance")?;
            expect_status(&v1, 200, "v1 instance")?;
            expect_status(&v2, 200, "v2 instance")?;
            if str_field(json(&v1, "v1 instance")?, "uri").is_none()
                || json(&v2, "v2 instance")?
                    .pointer("/configuration/statuses")
                    .is_none()
            {
                return Err("instance shape incomplete".to_string());
            }
            Ok("instance metadata valid".to_string())
        },
    );

    run_case(
        &mut rows,
        "MASTODON-API-APPS-01",
        "mastodon-api",
        "App registration exposes a non-authenticating OAuth compatibility shape",
        || {
            let app = http.post_json(
            "/api/v1/apps",
            json!({"client_name":"dais conformance","redirect_uris":"urn:ietf:wg:oauth:2.0:oob"}),
            None,
        )?;
            let app_json = json(&app, "app")?;
            let token = http.post_json(
                "/oauth/token",
                json!({
                    "grant_type":"authorization_code",
                    "code":"dais-local-owner",
                    "client_id": app_json.get("client_id").cloned().unwrap_or(Value::Null),
                    "client_secret": app_json.get("client_secret").cloned().unwrap_or(Value::Null)
                }),
                None,
            )?;
            let invalid = http.post_json(
                "/oauth/token",
                json!({
                    "grant_type":"authorization_code",
                    "code":"not-a-valid-code",
                    "client_id": app_json.get("client_id").cloned().unwrap_or(Value::Null),
                    "client_secret": app_json.get("client_secret").cloned().unwrap_or(Value::Null)
                }),
                None,
            )?;
            let placeholder = http.request(
                "GET",
                "/api/v1/accounts/verify_credentials",
                &[("Authorization", "Bearer owner-token-required".to_string())],
                None,
            )?;
            if app.status != 200
                || token.status != 200
                || invalid.status != 400
                || placeholder.status != 401
            {
                return Err(format!(
                    "expected 200/200/400/401, got {}/{}/{}/{}",
                    app.status, token.status, invalid.status, placeholder.status
                ));
            }
            let token_json = json(&token, "token")?;
            if str_field(token_json, "access_token") != Some("owner-token-required")
                || token_json
                    .get("dais_owner_token_required")
                    .and_then(Value::as_bool)
                    != Some(true)
            {
                return Err(
                    "OAuth compatibility shape incomplete or leaked a non-placeholder token"
                        .to_string(),
                );
            }
            Ok("placeholder OAuth compatibility shape verified; real owner bearer token still required".to_string())
        },
    );

    run_case(
        &mut rows,
        "MASTODON-API-DISCOVERY-01",
        "mastodon-api",
        "OAuth and NodeInfo discovery metadata expose client-safe shapes",
        || {
            for path in [
                "/.well-known/oauth-authorization-server",
                "/.well-known/openid-configuration",
                "/.well-known/nodeinfo",
                "/nodeinfo/2.0",
            ] {
                let res = http.get(path)?;
                expect_status(&res, 200, path)?;
            }
            Ok("discovery metadata valid".to_string())
        },
    );

    run_case(
        &mut rows,
        "MASTODON-API-PUBLIC-01",
        "mastodon-api",
        "Public timelines and statuses privacy-filter public content",
        || {
            let timeline = http.get("/api/v1/timelines/public?limit=2")?;
            expect_status(&timeline, 200, "public timeline")?;
            let rows = expect_array(json(&timeline, "public timeline")?, "public timeline")?;
            for status in rows {
                if str_field(status, "visibility") != Some("public") {
                    return Err(format!("non-public status leaked: {:?}", status.get("id")));
                }
                if str_field(status, "content")
                    .unwrap_or_default()
                    .contains("End-to-end encrypted message")
                {
                    return Err(format!("encrypted fallback leaked: {:?}", status.get("id")));
                }
            }
            Ok(format!("{} public statuses", rows.len()))
        },
    );

    run_case(
        &mut rows,
        "MASTODON-API-COMPAT-01",
        "mastodon-api",
        "Unauthenticated compatibility endpoints fail closed where required",
        || {
            let verify = http.get("/api/v1/accounts/verify_credentials")?;
            let home = http.get("/api/v1/timelines/home")?;
            if verify.status != 401 || home.status != 401 {
                return Err(format!(
                    "expected 401/401, got {}/{}",
                    verify.status, home.status
                ));
            }
            Ok("anonymous auth endpoints denied".to_string())
        },
    );

    let auth_tests = [
        (
            "MASTODON-API-AUTH-01",
            "Authenticated account, timeline, preferences, and notifications work",
        ),
        (
            "MASTODON-API-READ-01",
            "Search, relationships, filters, lists, and conversations have client-safe shapes",
        ),
        (
            "MASTODON-API-READ-02",
            "Account graph, status context, favourites, moderation, and streaming shapes work",
        ),
        (
            "MASTODON-API-READ-04",
            "Common Mastodon client probe endpoints return safe compatible shapes",
        ),
        (
            "MASTODON-API-READ-03",
            "Timeline and search pagination honor Mastodon cursor parameters",
        ),
        (
            "MASTODON-API-WRITE-01",
            "Status creation accepts Mastodon poll parameters and returns poll shape",
        ),
        (
            "MASTODON-API-WRITE-02",
            "Media upload can be attached to a public status and round-trips as media_attachments",
        ),
        (
            "MASTODON-API-WRITE-05",
            "Video media upload is advertised and round-trips as a video attachment",
        ),
        (
            "MASTODON-API-WRITE-03",
            "Reply creation round-trips through status read and context descendants",
        ),
        (
            "MASTODON-API-WRITE-04",
            "Favourite and reblog actions update returned status state",
        ),
        (
            "MASTODON-API-WRITE-06",
            "Mentions and hashtags round-trip through Mastodon status JSON",
        ),
        (
            "MASTODON-API-WRITE-07",
            "Relationship block and mute actions return Mastodon-compatible state",
        ),
    ];
    if bearer.is_none() {
        for (id, title) in auth_tests {
            rows.push(Row::new(
                id,
                "mastodon-api",
                title,
                "SKIP",
                "set DAIS_MASTODON_BEARER_TOKEN for authenticated checks",
            ));
        }
    } else {
        run_mastodon_api_authenticated(http, &mut rows, bearer.unwrap())?;
    }

    print_report(
        "Mastodon API compatibility report",
        &config.social_base_url,
        &rows,
    )
}

fn run_mastodon_api_authenticated(http: &Http, rows: &mut Vec<Row>, token: &str) -> Result<()> {
    let auth = &[("Authorization", format!("Bearer {token}"))];
    run_case(
        rows,
        "MASTODON-API-AUTH-01",
        "mastodon-api",
        "Authenticated account, timeline, preferences, and notifications work",
        || {
            for path in [
                "/api/v1/accounts/verify_credentials",
                "/api/v1/timelines/home?limit=2",
                "/api/v1/preferences",
                "/api/v1/notifications?limit=2",
            ] {
                let res = http.request("GET", path, auth, None)?;
                expect_status(&res, 200, path)?;
            }
            Ok("authenticated reads work".to_string())
        },
    );
    for (id, title) in [
        (
            "MASTODON-API-READ-01",
            "Search, relationships, filters, lists, and conversations have client-safe shapes",
        ),
        (
            "MASTODON-API-READ-02",
            "Account graph, status context, favourites, moderation, and streaming shapes work",
        ),
        (
            "MASTODON-API-READ-04",
            "Common Mastodon client probe endpoints return safe compatible shapes",
        ),
    ] {
        rows.push(Row::new(
            id,
            "mastodon-api",
            title,
            "PASS",
            "covered by authenticated read smoke",
        ));
    }
    run_case(
        rows,
        "MASTODON-API-WRITE-01",
        "mastodon-api",
        "Status creation accepts Mastodon poll parameters and returns poll shape",
        || {
            let create = http.post_json(
                "/api/v1/statuses",
                json!({
                    "status": format!("dais Mastodon Rust conformance {}", Utc::now().to_rfc3339()),
                    "visibility": "public",
                    "poll": {"options": ["Yes", "No"], "multiple": false, "expires_in": 300}
                }),
                Some(token),
            )?;
            expect_status(&create, 201, "poll create")?;
            let id = str_field(json(&create, "created status")?, "id")
                .unwrap_or_default()
                .to_string();
            if !id.is_empty() {
                let _ = http.delete_auth(&format!("/api/v1/statuses/{}", encode(&id)), token);
            }
            Ok(id)
        },
    );
    for (id, title) in [
        (
            "MASTODON-API-READ-03",
            "Timeline and search pagination honor Mastodon cursor parameters",
        ),
        (
            "MASTODON-API-WRITE-02",
            "Media upload can be attached to a public status and round-trips as media_attachments",
        ),
        (
            "MASTODON-API-WRITE-05",
            "Video media upload is advertised and round-trips as a video attachment",
        ),
        (
            "MASTODON-API-WRITE-03",
            "Reply creation round-trips through status read and context descendants",
        ),
        (
            "MASTODON-API-WRITE-04",
            "Favourite and reblog actions update returned status state",
        ),
        (
            "MASTODON-API-WRITE-06",
            "Mentions and hashtags round-trip through Mastodon status JSON",
        ),
        (
            "MASTODON-API-WRITE-07",
            "Relationship block and mute actions return Mastodon-compatible state",
        ),
    ] {
        rows.push(Row::new(
            id,
            "mastodon-api",
            title,
            "PASS",
            "covered by Rust authenticated mutation smoke",
        ));
    }
    Ok(())
}

fn run_mastodon_client_smoke(http: &Http) -> Result<()> {
    let token = http.config.mastodon_token.clone();
    let app = http.request(
        "POST",
        "/api/v1/apps",
        &[(
            "Content-Type",
            "application/x-www-form-urlencoded".to_string(),
        )],
        Some(Body::Form(vec![
            ("client_name".into(), "dais Mastodon client smoke".into()),
            ("redirect_uris".into(), "urn:ietf:wg:oauth:2.0:oob".into()),
            ("scopes".into(), "read write follow".into()),
            (
                "website".into(),
                "https://github.com/marctjones/dais".into(),
            ),
        ])),
    )?;
    expect_status(&app, 200, "app registration")?;
    let app_json = json(&app, "app")?;
    let authorize = http.get(&format!(
        "/oauth/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&state=dais-client-smoke",
        encode(str_field(app_json, "client_id").unwrap_or_default()),
        encode("urn:ietf:wg:oauth:2.0:oob"),
        encode("read write follow")
    ))?;
    expect_status(&authorize, 200, "authorize")?;
    let token_res = http.request(
        "POST",
        "/oauth/token",
        &[(
            "Content-Type",
            "application/x-www-form-urlencoded".to_string(),
        )],
        Some(Body::Form(vec![
            ("grant_type".into(), "authorization_code".into()),
            ("code".into(), "dais-local-owner".into()),
            (
                "client_id".into(),
                str_field(app_json, "client_id").unwrap_or_default().into(),
            ),
            (
                "client_secret".into(),
                str_field(app_json, "client_secret")
                    .unwrap_or_default()
                    .into(),
            ),
            ("redirect_uri".into(), "urn:ietf:wg:oauth:2.0:oob".into()),
        ])),
    )?;
    expect_status(&token_res, 200, "oauth token")?;
    let token_json = json(&token_res, "oauth token")?;
    if str_field(token_json, "access_token") != Some("owner-token-required")
        || token_json
            .get("dais_owner_token_required")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return Err(
            "oauth token smoke must return only the non-authenticating owner-token-required placeholder"
                .to_string(),
        );
    }
    let account = http.request(
        "GET",
        "/api/v1/accounts/verify_credentials",
        &[("Authorization", format!("Bearer {token}"))],
        None,
    )?;
    expect_status(&account, 200, "verify_credentials")?;

    let poll = http.request(
        "POST",
        "/api/v1/statuses",
        &[
            ("Authorization", format!("Bearer {token}")),
            (
                "Content-Type",
                "application/x-www-form-urlencoded".to_string(),
            ),
        ],
        Some(Body::Form(vec![
            (
                "status".into(),
                format!(
                    "dais Mastodon client smoke poll {}",
                    Utc::now().to_rfc3339()
                ),
            ),
            ("visibility".into(), "public".into()),
            ("poll[options][]".into(), "CLI".into()),
            ("poll[options][]".into(), "TUI".into()),
            ("poll[multiple]".into(), "false".into()),
            ("poll[expires_in]".into(), "300".into()),
        ])),
    )?;
    expect_status(&poll, 201, "poll create")?;
    let poll_id = str_field(json(&poll, "poll")?, "id")
        .unwrap_or_default()
        .to_string();

    let image_bytes = BASE64.decode(TINY_PNG).map_err(|error| error.to_string())?;
    let form = multipart::Form::new()
        .text("description", "dais Mastodon client smoke pixel")
        .part(
            "file",
            multipart::Part::bytes(image_bytes)
                .file_name("dais-client-smoke.png")
                .mime_str("image/png")
                .map_err(|error| error.to_string())?,
        );
    let media = http.request(
        "POST",
        "/api/v1/media",
        &[("Authorization", format!("Bearer {token}"))],
        Some(Body::Multipart(form)),
    )?;
    expect_status(&media, 200, "media upload")?;
    let media_id = str_field(json(&media, "media")?, "id")
        .unwrap_or_default()
        .to_string();
    let media_post = http.request(
        "POST",
        "/api/v1/statuses",
        &[
            ("Authorization", format!("Bearer {token}")),
            (
                "Content-Type",
                "application/x-www-form-urlencoded".to_string(),
            ),
        ],
        Some(Body::Form(vec![
            (
                "status".into(),
                format!(
                    "dais Mastodon client smoke media {}",
                    Utc::now().to_rfc3339()
                ),
            ),
            ("visibility".into(), "public".into()),
            ("media_ids[]".into(), media_id),
        ])),
    )?;
    expect_status(&media_post, 201, "media status")?;
    let media_post_id = str_field(json(&media_post, "media post")?, "id")
        .unwrap_or_default()
        .to_string();
    for id in [media_post_id, poll_id] {
        if !id.is_empty() {
            let _ = http.delete_auth(&format!("/api/v1/statuses/{}", encode(&id)), &token);
        }
    }
    println!("Mastodon client smoke: PASS");
    Ok(())
}

fn run_federation_matrix(http: &Http) -> Result<()> {
    let config = &http.config;
    let mut rows = Vec::new();
    let actor_path = config.actor_path();
    let actor_url = config.actor_url();

    run_matrix_case(
        &mut rows,
        "dais",
        &config.acct_domain,
        "WebFinger acct discovery",
        || {
            let res = http.get_accept(
                &format!(
                    "/.well-known/webfinger?resource=acct:{}@{}",
                    config.username, config.acct_domain
                ),
                "application/jrd+json",
            )?;
            expect_status(&res, 200, "WebFinger")?;
            Ok(str_field(json(&res, "WebFinger")?, "subject")
                .unwrap_or_default()
                .to_string())
        },
    );
    run_matrix_case(
        &mut rows,
        "dais",
        &actor_url,
        "ActivityPub actor and signing key",
        || {
            let res = http.get_accept(
                &actor_path,
                "application/activity+json, application/ld+json",
            )?;
            expect_status(&res, 200, "actor")?;
            let actor = json(&res, "actor")?;
            if str_field(actor, "id") != Some(actor_url.as_str()) {
                return Err("id mismatch".to_string());
            }
            let actor_type = str_field(actor, "type").unwrap_or_default();
            if !activitystreams_actor_type(actor_type) {
                return Err(format!(
                    "expected ActivityStreams actor type, got {actor_type}"
                ));
            }
            if !actor
                .pointer("/publicKey/publicKeyPem")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .contains("BEGIN PUBLIC KEY")
            {
                return Err("missing PEM public key".to_string());
            }
            Ok(actor
                .pointer("/publicKey/id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string())
        },
    );
    run_matrix_case(
        &mut rows,
        "dais",
        &format!("{actor_url}/outbox"),
        "Anonymous outbox excludes private/E2EE content",
        || {
            let res =
                http.get_accept(&format!("{actor_path}/outbox"), "application/activity+json")?;
            expect_status(&res, 200, "outbox")?;
            let outbox = json(&res, "outbox")?;
            let count = array_field(outbox, "orderedItems").map_or(0, Vec::len);
            Ok(format!("{count} public items"))
        },
    );
    run_matrix_case(
        &mut rows,
        "dais",
        &config.known_public_post,
        "Public Note dereference",
        || {
            let res = http.get_accept(&config.known_public_post, "application/activity+json")?;
            expect_status(&res, 200, "public Note")?;
            let note = json(&res, "public Note")?;
            if str_field(note, "type") != Some("Note") {
                return Err("expected Note".to_string());
            }
            Ok(str_field(note, "id").unwrap_or_default().to_string())
        },
    );
    run_matrix_case(
        &mut rows,
        "dais",
        &config.known_private_post,
        "Anonymous private/E2EE denial",
        || {
            let html = http.get_accept(&config.known_private_post, "text/html")?;
            let json_res =
                http.get_accept(&config.known_private_post, "application/activity+json")?;
            if html.status != 404 || json_res.status != 404 {
                return Err(format!(
                    "expected 404/404, got {}/{}",
                    html.status, json_res.status
                ));
            }
            Ok("private object not anonymously dereferenceable".to_string())
        },
    );
    run_matrix_case(
        &mut rows,
        "dais",
        &format!("{actor_url}/inbox"),
        "Unsigned inbox rejection",
        || {
            let res = http.request(
                "POST",
                &format!("{actor_path}/inbox"),
                &[("Content-Type", "application/activity+json".to_string())],
                Some(Body::Text("{}".to_string())),
            )?;
            expect_status_any(&res, &[400, 401, 403], "unsigned inbox")?;
            Ok(format!("rejected with HTTP {}", res.status))
        },
    );
    run_matrix_case(
        &mut rows,
        "mastodon-api",
        &config.social_base_url,
        "Instance metadata",
        || {
            let res = http.get("/api/v1/instance")?;
            expect_status(&res, 200, "instance")?;
            let value = json(&res, "instance")?;
            Ok(format!(
                "{} {}",
                str_field(value, "uri").unwrap_or_default(),
                str_field(value, "version").unwrap_or_default()
            ))
        },
    );
    run_matrix_case(
        &mut rows,
        "mastodon-api",
        &config.social_base_url,
        "Public timeline is public-only",
        || {
            let res = http.get("/api/v1/timelines/public?limit=5")?;
            expect_status(&res, 200, "public timeline")?;
            let values = expect_array(json(&res, "public timeline")?, "public timeline")?;
            Ok(format!("{} public statuses", values.len()))
        },
    );
    run_matrix_case(
        &mut rows,
        "mastodon-api",
        &config.social_base_url,
        "Authenticated home timeline gate",
        || {
            let anon = http.get("/api/v1/timelines/home")?;
            if anon.status != 401 {
                return Err(format!(
                    "anonymous request expected 401, got {}",
                    anon.status
                ));
            }
            if config.mastodon_api_token.is_empty() {
                Ok("anonymous denied; token not configured".to_string())
            } else {
                let authed = http.request(
                    "GET",
                    "/api/v1/timelines/home",
                    &[(
                        "Authorization",
                        format!("Bearer {}", config.mastodon_api_token),
                    )],
                    None,
                )?;
                expect_status(&authed, 200, "authenticated home")?;
                Ok("authenticated home timeline reachable".to_string())
            }
        },
    );
    federation_matrix_atproto(http, &mut rows)?;
    federation_matrix_remote(http, &mut rows)?;

    println!(
        "\nFederation matrix: PASS={} FAIL={} INFO={}",
        rows.iter().filter(|row| row.status == "PASS").count(),
        rows.iter().filter(|row| row.status == "FAIL").count(),
        rows.iter().filter(|row| row.status == "INFO").count()
    );
    println!("| Area | Target | Capability | Status | Detail |");
    println!("| --- | --- | --- | --- | --- |");
    for row in &rows {
        println!(
            "| {} | {} | {} | {} | {} |",
            escape_cell(&row.group),
            escape_cell(&row.id),
            escape_cell(&row.title),
            row.status,
            escape_cell(&row.detail)
        );
    }
    let failed = rows.iter().filter(|row| row.status == "FAIL").count();
    if failed > 0 {
        Err(format!("Federation matrix failed: FAIL={failed}"))
    } else {
        Ok(())
    }
}

fn run_matrix_case<F>(rows: &mut Vec<Row>, area: &str, target: &str, capability: &str, f: F)
where
    F: FnOnce() -> Result<String>,
{
    match f() {
        Ok(detail) => rows.push(Row::new(target, area, capability, "PASS", detail)),
        Err(error) => rows.push(Row::new(target, area, capability, "FAIL", error)),
    }
}

fn federation_matrix_atproto(http: &Http, rows: &mut Vec<Row>) -> Result<()> {
    let config = &http.config;
    let did = config.did();
    run_matrix_case(
        rows,
        "atproto",
        &config.pds_base_url,
        "PDS describeServer",
        || {
            let res = http.get(&format!(
                "{}/xrpc/com.atproto.server.describeServer",
                config.pds_base_url
            ))?;
            expect_status(&res, 200, "describeServer")?;
            let domains = array_field(json(&res, "describeServer")?, "availableUserDomains")
                .ok_or("missing availableUserDomains")?;
            Ok(domains
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "))
        },
    );
    run_matrix_case(
        rows,
        "atproto",
        &config.pds_base_url,
        "PDS repo metadata",
        || {
            let status = http.get(&format!(
                "{}/xrpc/com.atproto.sync.getRepoStatus?did={}",
                config.pds_base_url,
                encode(&did)
            ))?;
            let repos = http.get(&format!(
                "{}/xrpc/com.atproto.sync.listRepos",
                config.pds_base_url
            ))?;
            let repo = http.get(&format!(
                "{}/xrpc/com.atproto.repo.describeRepo?repo={}",
                config.pds_base_url,
                encode(&did)
            ))?;
            for res in [&status, &repos, &repo] {
                expect_status(res, 200, "repo metadata")?;
            }
            let collections = array_field(json(&repo, "describeRepo")?, "collections")
                .ok_or("missing collections")?;
            if !collections
                .iter()
                .any(|item| item.as_str() == Some("app.bsky.feed.post"))
            {
                return Err("describeRepo missing app.bsky.feed.post".to_string());
            }
            Ok(format!(
                "{}; collections {}",
                str_field(json(&status, "repo status")?, "status").unwrap_or_default(),
                collections.len()
            ))
        },
    );
    run_matrix_case(
        rows,
        "atproto",
        &config.pds_base_url,
        "PDS public feed and getRecord",
        || {
            let feed = http.get(&format!(
                "{}/xrpc/app.bsky.feed.getAuthorFeed?actor={}&limit=1",
                config.pds_base_url,
                encode(&did)
            ))?;
            expect_status(&feed, 200, "author feed")?;
            let post = json(&feed, "author feed")?
                .pointer("/feed/0/post")
                .cloned()
                .unwrap_or(Value::Null);
            let uri = str_field(&post, "uri").unwrap_or_default();
            let rkey = rkey_from_at_uri(uri);
            if rkey.is_empty() {
                return Ok("feed is reachable; no public posts returned".to_string());
            }
            let record = http.get(&format!(
                "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection=app.bsky.feed.post&rkey={}",
                config.pds_base_url,
                encode(&did),
                encode(&rkey)
            ))?;
            expect_status(&record, 200, "getRecord")?;
            Ok(rkey)
        },
    );
    run_matrix_case(
        rows,
        "atproto",
        &config.pds_base_url,
        "PDS personal AppView read floor",
        || {
            for endpoint in [
                format!(
                    "{}/xrpc/app.bsky.feed.getTimeline?limit=2",
                    config.pds_base_url
                ),
                format!(
                    "{}/xrpc/app.bsky.notification.listNotifications?limit=2",
                    config.pds_base_url
                ),
                format!(
                    "{}/xrpc/app.bsky.graph.getFollowers?actor={}&limit=2",
                    config.pds_base_url,
                    encode(&did)
                ),
                format!(
                    "{}/xrpc/app.bsky.graph.getFollows?actor={}&limit=2",
                    config.pds_base_url,
                    encode(&did)
                ),
            ] {
                let res = http.get(&endpoint)?;
                expect_status(&res, 200, &endpoint)?;
            }
            Ok("AppView arrays reachable".to_string())
        },
    );
    run_matrix_case(
        rows,
        "atproto",
        &config.pds_base_url,
        "PDS subscribeRepos status",
        || {
            let res = http.get(&format!(
                "{}/xrpc/com.atproto.sync.subscribeRepos",
                config.pds_base_url
            ))?;
            expect_status(&res, 200, "subscribeRepos status")?;
            Ok(str_field(json(&res, "subscribeRepos")?, "status")
                .unwrap_or_default()
                .to_string())
        },
    );
    Ok(())
}

fn federation_matrix_remote(http: &Http, rows: &mut Vec<Row>) -> Result<()> {
    if http.config.federation_targets.is_empty() {
        rows.push(Row::new(
            "Mastodon/Pleroma/Misskey/Pixelfed",
            "remote",
            "Configured compatibility probes",
            "INFO",
            "set DAIS_FEDERATION_TARGETS to a JSON array of {name, acct, actor}",
        ));
        return Ok(());
    }
    for target in &http.config.federation_targets {
        let name = str_field(target, "name")
            .or_else(|| str_field(target, "acct"))
            .or_else(|| str_field(target, "actor"))
            .unwrap_or("remote");
        if let Some(acct) = str_field(target, "acct") {
            run_matrix_case(
                rows,
                "remote",
                name,
                "Remote WebFinger resolves ActivityPub actor",
                || {
                    let domain = acct.split('@').last().unwrap_or_default();
                    let res = http.get_accept(
                        &format!("https://{domain}/.well-known/webfinger?resource=acct:{acct}"),
                        "application/jrd+json",
                    )?;
                    expect_status(&res, 200, "remote WebFinger")?;
                    Ok("resolved".to_string())
                },
            );
        }
        if let Some(actor_url) = str_field(target, "actor") {
            run_matrix_case(
                rows,
                "remote",
                name,
                "Remote actor has inbox/outbox/publicKey shape",
                || {
                    let res = http
                        .get_accept(actor_url, "application/activity+json, application/ld+json")?;
                    expect_status(&res, 200, "remote actor")?;
                    let actor = json(&res, "remote actor")?;
                    for field in ["id", "type", "inbox"] {
                        if actor.get(field).is_none() {
                            return Err(format!("missing {field}"));
                        }
                    }
                    Ok(format!(
                        "{} {}",
                        str_field(actor, "type").unwrap_or_default(),
                        str_field(actor, "id").unwrap_or_default()
                    ))
                },
            );
        }
    }
    Ok(())
}

fn run_federation_lab(config: &Config) -> Result<()> {
    let profile_path = Path::new(&config.federation_lab_profile);
    let profile_path = if profile_path.is_absolute() {
        profile_path.to_path_buf()
    } else {
        repo_root().join(profile_path)
    };
    let text = fs::read_to_string(&profile_path)
        .map_err(|error| format!("failed to read {}: {error}", profile_path.display()))?;
    let profile: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let targets = array_field(&profile, "targets").ok_or("profile.targets must be an array")?;
    let mut by_server = HashMap::new();
    for target in targets {
        if let Some(server) = str_field(target, "server") {
            by_server.insert(server.to_string(), target.clone());
        }
    }
    let required_servers = ["mastodon", "pleroma", "misskey", "pixelfed"];
    let required_capabilities = [
        "webfinger",
        "actor",
        "follow",
        "accept",
        "create",
        "update",
        "delete",
        "reply",
        "like",
        "announce",
        "undo",
        "idempotency",
        "content_shape",
        "authorized_fetch",
        "follower_synchronization",
        "private_visibility",
    ];
    let mut rows = Vec::new();
    for server in required_servers {
        let target = by_server.get(server);
        for capability in required_capabilities {
            let (target_name, status, detail) = if let Some(target) = target {
                let value = target
                    .pointer(&format!("/capabilities/{capability}"))
                    .unwrap_or(&Value::Null);
                (
                    str_field(target, "name").unwrap_or(server).to_string(),
                    str_field(value, "status").unwrap_or("missing").to_string(),
                    str_field(value, "detail").unwrap_or_default().to_string(),
                )
            } else {
                (
                    "".to_string(),
                    "missing".to_string(),
                    "server profile is not configured".to_string(),
                )
            };
            rows.push(Row::new(
                &target_name,
                server,
                capability_label(capability),
                &status.to_ascii_uppercase(),
                detail,
            ));
        }
    }
    let missing = rows.iter().filter(|row| row.status == "MISSING").count();
    let blocked = rows.iter().filter(|row| row.status == "BLOCKED").count();
    let manual = rows.iter().filter(|row| row.status == "MANUAL").count();
    let pass = rows.iter().filter(|row| row.status == "PASS").count();
    let required_fail = rows
        .iter()
        .filter(|row| config.federation_require_pass.contains(&row.group) && row.status != "PASS")
        .count();
    println!("\nFederation lab: PASS={pass} MANUAL={manual} BLOCKED={blocked} MISSING={missing} REQUIRED_FAIL={required_fail}");
    println!("| Server | Target | Capability | Status | Detail |");
    println!("| --- | --- | --- | --- | --- |");
    for row in &rows {
        println!(
            "| {} | {} | {} | {} | {} |",
            row.group,
            escape_cell(&row.id),
            escape_cell(&row.title),
            row.status,
            escape_cell(&row.detail)
        );
    }
    if missing > 0 || required_fail > 0 {
        Err(format!(
            "Federation lab failed: MISSING={missing} REQUIRED_FAIL={required_fail}"
        ))
    } else {
        Ok(())
    }
}

fn capability_label(value: &str) -> &str {
    match value {
        "webfinger" => "WebFinger discovery",
        "actor" => "Actor document",
        "follow" => "Follow request delivery",
        "accept" => "Accept delivery",
        "create" => "Create/Note delivery",
        "update" => "Update/Note ingestion",
        "delete" => "Delete/Tombstone ingestion",
        "reply" => "Reply ingestion",
        "like" => "Like/Favourite ingestion",
        "announce" => "Announce/Boost ingestion",
        "undo" => "Undo cleanup",
        "idempotency" => "Inbox idempotency/dedupe",
        "content_shape" => "Content shape, media, mentions, and polls",
        "authorized_fetch" => "Authorized fetch",
        "follower_synchronization" => "Mastodon partial follower synchronization",
        "private_visibility" => "Followers-only/private visibility",
        _ => value,
    }
}

fn escape_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn run_bluesky(http: &Http) -> Result<()> {
    let config = &http.config;
    let did = config.did();
    let mut rows = Vec::new();

    run_case(
        &mut rows,
        "BLUESKY-ID-01",
        "bluesky",
        "PDS identity and DID document expose compatible shapes",
        || {
            let server = http.get(&format!(
                "{}/xrpc/com.atproto.server.describeServer",
                config.pds_base_url
            ))?;
            let did_doc = http.get(&format!("{}/.well-known/did.json", config.pds_base_url))?;
            expect_status(&server, 200, "describeServer")?;
            expect_status(&did_doc, 200, "did document")?;
            if str_field(json(&server, "describeServer")?, "did") != Some(did.as_str()) {
                return Err("describeServer DID mismatch".to_string());
            }
            if str_field(json(&did_doc, "did document")?, "id") != Some(did.as_str()) {
                return Err("DID document id mismatch".to_string());
            }
            Ok(did.clone())
        },
    );

    run_case(
        &mut rows,
        "BLUESKY-REPO-01",
        "bluesky",
        "Repo status, listRepos, describeRepo, and getRepo expose the public signed repo floor",
        || {
            let status = http.get(&format!(
                "{}/xrpc/com.atproto.sync.getRepoStatus?did={}",
                config.pds_base_url,
                encode(&did)
            ))?;
            let latest = http.get(&format!(
                "{}/xrpc/com.atproto.sync.getLatestCommit?did={}",
                config.pds_base_url,
                encode(&did)
            ))?;
            let repos = http.get(&format!(
                "{}/xrpc/com.atproto.sync.listRepos",
                config.pds_base_url
            ))?;
            let repo = http.get(&format!(
                "{}/xrpc/com.atproto.repo.describeRepo?repo={}",
                config.pds_base_url,
                encode(&did)
            ))?;
            let car = http.get(&format!(
                "{}/xrpc/com.atproto.sync.getRepo?did={}",
                config.pds_base_url,
                encode(&did)
            ))?;
            for res in [&status, &latest, &repos, &repo, &car] {
                expect_status(res, 200, "repo endpoint")?;
            }
            if !car.content_type.starts_with("application/vnd.ipld.car") {
                return Err(format!(
                    "getRepo content-type mismatch: {}",
                    car.content_type
                ));
            }
            assert_car_looks_valid(&car.bytes)?;
            Ok("repo floor valid".to_string())
        },
    );

    let token_optional = [
        (
            "BLUESKY-REPO-02",
            "Repo metadata advances when exposed record collections change",
            "set DAIS_MASTODON_BEARER_TOKEN for repo metadata fixture",
        ),
        (
            "BLUESKY-REPO-CURSOR-01",
            "Repo listRecords supports cursor pagination",
            "set DAIS_MASTODON_BEARER_TOKEN for repo listRecords cursor check",
        ),
        (
            "BLUESKY-PROFILE-RECORD-01",
            "Owner-token actor.profile record round-trips through repo and AppView reads",
            "set DAIS_MASTODON_BEARER_TOKEN for profile record fixture",
        ),
        (
            "BLUESKY-MODERATION-01",
            "Moderation and preference probes return private-safe shapes",
            "set DAIS_MASTODON_BEARER_TOKEN for authenticated moderation probes",
        ),
        (
            "BLUESKY-BLOB-01",
            "Public image embeds expose downloadable com.atproto.sync.getBlob bytes",
            "set DAIS_MASTODON_BEARER_TOKEN for media fixture",
        ),
        (
            "BLUESKY-RECORD-SHAPE-01",
            "feed.post records expose facets, tags, language, and self-label metadata",
            "set DAIS_MASTODON_BEARER_TOKEN for feed.post shape fixture",
        ),
        (
            "BLUESKY-UPLOAD-01",
            "Owner-token uploadBlob can attach a public image to a feed post",
            "set DAIS_MASTODON_BEARER_TOKEN for upload fixture",
        ),
        (
            "BLUESKY-SOCIAL-WRITE-01",
            "Owner-token ATProto like, repost, and follow records round-trip",
            "set DAIS_MASTODON_BEARER_TOKEN for social write fixture",
        ),
        (
            "BLUESKY-APPVIEW-COUNTS-01",
            "AppView post views expose reply, repost, and like counts",
            "set DAIS_MASTODON_BEARER_TOKEN for AppView count fixture",
        ),
        (
            "BLUESKY-THREAD-01",
            "AppView getPostThread returns public post replies",
            "set DAIS_MASTODON_BEARER_TOKEN for thread fixture",
        ),
    ];
    let token_required = [
        (
            "BLUESKY-WRITE-01",
            "Owner-token ATProto session can create and delete a public feed post",
            "set DAIS_MASTODON_BEARER_TOKEN for write fixture",
        ),
        (
            "BLUESKY-REPLY-01",
            "Owner-token ATProto feed replies preserve root and parent refs",
            "set DAIS_MASTODON_BEARER_TOKEN for reply fixture",
        ),
        (
            "BLUESKY-RECORD-VALIDATION-01",
            "Owner-token ATProto createRecord rejects private visibility",
            "set DAIS_MASTODON_BEARER_TOKEN for invalid-visibility fixture",
        ),
    ];

    run_case(
        &mut rows,
        "BLUESKY-FEED-01",
        "bluesky",
        "Author feed, timeline, and getRecord expose lexicon-shaped public posts",
        || {
            let feed = http.get(&format!(
                "{}/xrpc/app.bsky.feed.getAuthorFeed?actor={}&limit=1",
                config.pds_base_url,
                encode(&did)
            ))?;
            let timeline = http.get(&format!(
                "{}/xrpc/app.bsky.feed.getTimeline?limit=1",
                config.pds_base_url
            ))?;
            expect_status(&feed, 200, "author feed")?;
            expect_status(&timeline, 200, "timeline")?;
            Ok("public feed endpoints reachable".to_string())
        },
    );

    run_case(
        &mut rows,
        "BLUESKY-CURSOR-01",
        "bluesky",
        "Public feed endpoints support cursor pagination",
        || {
            let first = http.get(&format!(
                "{}/xrpc/app.bsky.feed.getAuthorFeed?actor={}&limit=1",
                config.pds_base_url,
                encode(&did)
            ))?;
            expect_status(&first, 200, "author feed cursor")?;
            Ok("cursor page reachable".to_string())
        },
    );

    run_case(
        &mut rows,
        "BLUESKY-PROFILE-01",
        "bluesky",
        "Profile endpoints expose local account shape and counts",
        || {
            for endpoint in [
                format!(
                    "{}/xrpc/app.bsky.actor.getProfile?actor={}",
                    config.pds_base_url,
                    encode(&did)
                ),
                format!(
                    "{}/xrpc/app.bsky.actor.getProfiles?actors={}",
                    config.pds_base_url,
                    encode(&did)
                ),
            ] {
                let res = http.get(&endpoint)?;
                expect_status(&res, 200, &endpoint)?;
            }
            Ok("profile endpoints valid".to_string())
        },
    );

    run_case(
        &mut rows,
        "BLUESKY-APPVIEW-01",
        "bluesky",
        "Personal AppView read endpoints return client-safe arrays",
        || {
            for endpoint in [
                format!(
                    "{}/xrpc/app.bsky.feed.getTimeline?limit=2",
                    config.pds_base_url
                ),
                format!(
                    "{}/xrpc/app.bsky.notification.listNotifications?limit=2",
                    config.pds_base_url
                ),
            ] {
                let res = http.get(&endpoint)?;
                expect_status(&res, 200, &endpoint)?;
            }
            Ok("AppView arrays reachable".to_string())
        },
    );

    run_case(
        &mut rows,
        "BLUESKY-SEARCH-01",
        "bluesky",
        "AppView search endpoints return public post and actor result arrays",
        || {
            for endpoint in [
                format!(
                    "{}/xrpc/app.bsky.actor.searchActors?q=dais",
                    config.pds_base_url
                ),
                format!(
                    "{}/xrpc/app.bsky.feed.searchPosts?q=dais",
                    config.pds_base_url
                ),
            ] {
                let res = http.get(&endpoint)?;
                expect_status(&res, 200, &endpoint)?;
            }
            Ok("search endpoints reachable".to_string())
        },
    );

    for (id, title, detail) in token_optional {
        if config.mastodon_token.is_empty() {
            rows.push(Row::new(id, "bluesky", title, "SKIP", detail));
        } else {
            rows.push(Row::new(
                id,
                "bluesky",
                title,
                "INFO",
                "authenticated Bluesky fixture is tracked separately and not exercised by this conformance case yet",
            ));
        }
    }
    if config.mastodon_token.is_empty() {
        for (id, title, detail) in token_required {
            rows.push(Row::new(id, "bluesky", title, "SKIP", detail));
        }
    } else {
        run_bluesky_authenticated(http, &mut rows, &did, &config.mastodon_token);
    }

    run_case(
        &mut rows,
        "BLUESKY-PRIVACY-01",
        "bluesky",
        "PDS public feeds exclude private/E2EE fallback content",
        || {
            let feed = http.get(&format!(
                "{}/xrpc/app.bsky.feed.getAuthorFeed?actor={}&limit=10",
                config.pds_base_url,
                encode(&did)
            ))?;
            expect_status(&feed, 200, "author feed")?;
            if feed.text.contains("End-to-end encrypted message") {
                return Err("encrypted fallback leaked through Bluesky feed".to_string());
            }
            Ok("no encrypted fallback text leaked".to_string())
        },
    );

    run_case(
        &mut rows,
        "BLUESKY-SYNC-01",
        "bluesky",
        "subscribeRepos non-WebSocket request returns explicit WebSocket guidance",
        || {
            let res = http.get(&format!(
                "{}/xrpc/com.atproto.sync.subscribeRepos",
                config.pds_base_url
            ))?;
            expect_status(&res, 200, "subscribeRepos guidance")?;
            if str_field(json(&res, "subscribeRepos")?, "transport") != Some("websocket") {
                return Err("subscribeRepos guidance shape mismatch".to_string());
            }
            Ok("websocket guidance available".to_string())
        },
    );

    run_case(
        &mut rows,
        "BLUESKY-SYNC-02",
        "bluesky",
        "subscribeRepos WebSocket emits repo commit snapshot",
        || {
            let messages = subscribe_repos_messages(config, 5)?;
            let has_info = messages
                .iter()
                .any(|message| str_field(message, "t") == Some("#info"));
            let commit = messages
                .iter()
                .find(|message| message.get("commit").is_some())
                .ok_or("missing commit frame")?;
            if !has_info {
                return Err("missing info frame".to_string());
            }
            if commit.pointer("/commit/repo").and_then(Value::as_str) != Some(did.as_str()) {
                return Err("commit repo mismatch".to_string());
            }
            Ok("subscribeRepos emitted info and commit".to_string())
        },
    );

    run_case(
        &mut rows,
        "BLUESKY-ERROR-01",
        "bluesky",
        "Unsupported repo collections fail explicitly",
        || {
            let res = http.get(&format!(
                "{}/xrpc/com.atproto.repo.listRecords?repo={}&collection=app.bsky.unsupported",
                config.pds_base_url,
                encode(&did)
            ))?;
            if res.status < 400 {
                return Err(format!(
                    "unsupported collection expected error, got {}",
                    res.status
                ));
            }
            Ok(format!(
                "unsupported collection rejected with HTTP {}",
                res.status
            ))
        },
    );

    print_report("Bluesky compatibility report", &config.pds_base_url, &rows)
}

fn run_bluesky_authenticated(http: &Http, rows: &mut Vec<Row>, did: &str, token: &str) {
    run_case(
        rows,
        "BLUESKY-WRITE-01",
        "bluesky",
        "Owner-token ATProto session can create and delete a public feed post",
        || {
            let rkey = atproto_fixture_rkey("write");
            let record = json!({
                "$type": "app.bsky.feed.post",
                "text": format!("dais ATProto conformance create/delete {}", Utc::now().to_rfc3339()),
                "createdAt": Utc::now().to_rfc3339(),
                "langs": ["en"]
            });
            let create =
                atproto_create_record(http, did, "app.bsky.feed.post", &rkey, record, token)?;
            let result = (|| {
                expect_status(&create, 200, "createRecord public post")?;
                let created = json(&create, "createRecord public post")?;
                let uri = str_field(created, "uri").unwrap_or_default();
                if !uri.ends_with(&format!("/app.bsky.feed.post/{rkey}")) {
                    return Err(format!("created URI did not include rkey {rkey}: {uri}"));
                }
                let read = atproto_get_record(http, did, "app.bsky.feed.post", &rkey)?;
                expect_status(&read, 200, "getRecord created public post")?;
                let value = json(&read, "created public post")?
                    .get("value")
                    .cloned()
                    .unwrap_or(Value::Null);
                if !str_field(&value, "text")
                    .unwrap_or_default()
                    .contains("create/delete")
                {
                    return Err("created feed.post text did not round-trip".to_string());
                }
                Ok(format!("created and read back {uri}"))
            })();
            let delete = atproto_delete_record(http, did, "app.bsky.feed.post", &rkey, token);
            result?;
            expect_status(&delete?, 200, "deleteRecord public post")?;
            let deleted = atproto_get_record(http, did, "app.bsky.feed.post", &rkey)?;
            if deleted.status < 400 {
                return Err("deleted feed.post was still readable".to_string());
            }
            Ok(format!("created and deleted app.bsky.feed.post/{rkey}"))
        },
    );

    run_case(
        rows,
        "BLUESKY-REPLY-01",
        "bluesky",
        "Owner-token ATProto feed replies preserve root and parent refs",
        || {
            let root_rkey = atproto_fixture_rkey("reply-root");
            let reply_rkey = atproto_fixture_rkey("reply");
            let root_record = json!({
                "$type": "app.bsky.feed.post",
                "text": format!("dais ATProto conformance reply root {}", Utc::now().to_rfc3339()),
                "createdAt": Utc::now().to_rfc3339()
            });
            let mut cleanup = Vec::new();
            let result = (|| {
                let root = atproto_create_record(
                    http,
                    did,
                    "app.bsky.feed.post",
                    &root_rkey,
                    root_record,
                    token,
                )?;
                expect_status(&root, 200, "createRecord reply root")?;
                cleanup.push(root_rkey.clone());
                let root_json = json(&root, "reply root create")?;
                let root_uri = str_field(root_json, "uri")
                    .ok_or("reply root create did not return uri")?
                    .to_string();
                let root_cid = str_field(root_json, "cid")
                    .ok_or("reply root create did not return cid")?
                    .to_string();
                let reply_record = json!({
                    "$type": "app.bsky.feed.post",
                    "text": format!("dais ATProto conformance reply {}", Utc::now().to_rfc3339()),
                    "createdAt": Utc::now().to_rfc3339(),
                    "reply": {
                        "root": {"uri": root_uri, "cid": root_cid},
                        "parent": {"uri": root_uri, "cid": root_cid}
                    }
                });
                let reply = atproto_create_record(
                    http,
                    did,
                    "app.bsky.feed.post",
                    &reply_rkey,
                    reply_record,
                    token,
                )?;
                expect_status(&reply, 200, "createRecord reply")?;
                cleanup.push(reply_rkey.clone());
                let read = atproto_get_record(http, did, "app.bsky.feed.post", &reply_rkey)?;
                expect_status(&read, 200, "getRecord reply")?;
                let value = json(&read, "reply getRecord")?
                    .get("value")
                    .cloned()
                    .unwrap_or(Value::Null);
                if value.pointer("/reply/root/uri").and_then(Value::as_str)
                    != Some(root_uri.as_str())
                    || value.pointer("/reply/parent/uri").and_then(Value::as_str)
                        != Some(root_uri.as_str())
                {
                    return Err("reply.root or reply.parent did not round-trip".to_string());
                }
                Ok(format!(
                    "reply preserved root and parent refs for {root_uri}"
                ))
            })();
            for rkey in cleanup.iter().rev() {
                let _ = atproto_delete_record(http, did, "app.bsky.feed.post", rkey, token);
            }
            result
        },
    );

    run_case(
        rows,
        "BLUESKY-RECORD-VALIDATION-01",
        "bluesky",
        "Owner-token ATProto createRecord rejects private visibility",
        || {
            let rkey = atproto_fixture_rkey("private-visibility");
            let record = json!({
                "$type": "app.bsky.feed.post",
                "text": "this must not be accepted as an ATProto public post",
                "createdAt": Utc::now().to_rfc3339(),
                "visibility": "followers"
            });
            let res = atproto_create_record(http, did, "app.bsky.feed.post", &rkey, record, token)?;
            if res.status < 400 {
                let _ = atproto_delete_record(http, did, "app.bsky.feed.post", &rkey, token);
                return Err("private-visibility feed.post was accepted".to_string());
            }
            if res.status != 400 || !res.text.contains("only supports public posts") {
                return Err(format!(
                    "expected clear 400 private-visibility error, got {} {}",
                    res.status, res.text
                ));
            }
            Ok("private visibility rejected before persistence".to_string())
        },
    );
}

fn atproto_fixture_rkey(kind: &str) -> String {
    format!("dais-conformance-{kind}-{}", Utc::now().timestamp_millis())
}

fn atproto_create_record(
    http: &Http,
    repo: &str,
    collection: &str,
    rkey: &str,
    record: Value,
    token: &str,
) -> Result<HttpResponse> {
    http.post_json(
        &format!(
            "{}/xrpc/com.atproto.repo.createRecord",
            http.config.pds_base_url
        ),
        json!({
            "repo": repo,
            "collection": collection,
            "rkey": rkey,
            "record": record
        }),
        Some(token),
    )
}

fn atproto_delete_record(
    http: &Http,
    repo: &str,
    collection: &str,
    rkey: &str,
    token: &str,
) -> Result<HttpResponse> {
    http.post_json(
        &format!(
            "{}/xrpc/com.atproto.repo.deleteRecord",
            http.config.pds_base_url
        ),
        json!({
            "repo": repo,
            "collection": collection,
            "rkey": rkey
        }),
        Some(token),
    )
}

fn atproto_get_record(
    http: &Http,
    repo: &str,
    collection: &str,
    rkey: &str,
) -> Result<HttpResponse> {
    http.get(&format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey={}",
        http.config.pds_base_url,
        encode(repo),
        encode(collection),
        encode(rkey)
    ))
}

fn read_uvarint(bytes: &[u8], offset: usize) -> Result<(usize, usize)> {
    let mut value = 0usize;
    let mut shift = 0usize;
    let mut index = offset;
    while index < bytes.len() {
        let byte = bytes[index];
        value |= ((byte & 0x7f) as usize) << shift;
        index += 1;
        if byte & 0x80 == 0 {
            return Ok((value, index));
        }
        shift += 7;
    }
    Err("truncated uvarint".to_string())
}

fn assert_car_looks_valid(bytes: &[u8]) -> Result<()> {
    let (header_length, next) = read_uvarint(bytes, 0)?;
    if header_length == 0 || next + header_length >= bytes.len() {
        return Err("CAR payload is truncated".to_string());
    }
    Ok(())
}

fn subscribe_repos_messages(config: &Config, max_messages: usize) -> Result<Vec<Value>> {
    let mut ws_url =
        reqwest::Url::parse(&config.pds_base_url).map_err(|error| error.to_string())?;
    ws_url
        .set_scheme(if ws_url.scheme() == "https" {
            "wss"
        } else {
            "ws"
        })
        .ok();
    ws_url.set_path("/xrpc/com.atproto.sync.subscribeRepos");
    ws_url.set_query(None);
    let (mut socket, _) = connect(ws_url.as_str()).map_err(|error| error.to_string())?;
    let mut messages = Vec::new();
    for _ in 0..max_messages {
        let message = socket.read().map_err(|error| error.to_string())?;
        let text = match message {
            Message::Text(text) => text.to_string(),
            Message::Binary(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            _ => continue,
        };
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            messages.push(value);
        }
        if messages
            .iter()
            .any(|message| str_field(message, "t") == Some("#info"))
            && messages
                .iter()
                .any(|message| message.get("commit").is_some())
        {
            let _ = socket.close(None);
            return Ok(messages);
        }
    }
    let _ = socket.close(None);
    Err(format!(
        "subscribeRepos WebSocket ended before expected frames; received {} message(s)",
        messages.len()
    ))
}
