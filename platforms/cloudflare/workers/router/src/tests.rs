use super::{
    activitypub_actor_profile_html, activitypub_watch_item, audience_group_purpose_label,
    audience_membership_label, bluesky_actor_target, bluesky_appview_xrpc_url, bluesky_post_uri,
    bluesky_watch_item, display_local_url, e2ee_device_fingerprint,
    encrypted_media_attachments_from_activitypub_object, is_local_object_url,
    is_public_atproto_image_attachment, media_custom_metadata, normalize_ai_categories,
    normalize_audience_group_type, normalize_audience_membership_visibility,
    normalize_audience_posting_policy, normalize_discovered_public_post, normalize_e2ee_device_id,
    normalize_e2ee_fingerprint, normalize_e2ee_protocol, normalize_encrypted_media_attachments,
    normalize_owner_post_attachments, normalized_source_target, owner_normalize_bluesky_post,
    owner_normalize_tootfinder_status, owner_public_post_row_from_discovered,
    owner_public_search_mastodon_query_params, owner_token_has_scopes, parse_lenient_json_body,
    parse_scoped_owner_tokens, parse_workers_ai_moderation, peer_trust_state_after_material_update,
    sha256_hex, source_id, source_policy_json_for_type, source_type_for_watch_kind,
    strip_json_fence, tootfinder_search_items, tootfinder_search_url,
    validate_dais_encrypted_message_v2, validate_e2ee_device_material,
    validate_encrypted_media_payload, validate_owner_e2ee_payload, MediaMetadataInput,
    OwnerProfile, OwnerPublicSearchOptions, OwnerPublicSearchProvider, OwnerPublicSearchResultType,
    SourcePolicy, PUBLIC_COLLECTION,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::{Map, Value};

#[test]
fn actor_html_profile_includes_public_posts() {
    let profile = OwnerProfile {
        id: "https://social.dais.social/users/social".to_string(),
        username: "social".to_string(),
        actor_type: "Organization".to_string(),
        display_name: Some("dais".to_string()),
        summary: Some("<private-by-default>".to_string()),
        icon: None,
        image: None,
        avatar_url: None,
        header_url: None,
        public_handle: "@social@dais.social".to_string(),
        actor_url: "https://social.dais.social/users/social".to_string(),
    };
    let mut post = Map::new();
    post.insert(
        "id".to_string(),
        Value::String("https://social.dais.social/users/social/posts/1".to_string()),
    );
    post.insert(
        "content_html".to_string(),
        Value::String("<p>Visible public update</p>".to_string()),
    );
    post.insert(
        "published_at".to_string(),
        Value::String("2026-06-24T12:00:00Z".to_string()),
    );
    let html = activitypub_actor_profile_html(&profile, &[post]);
    assert!(html.contains("Public posts"));
    assert!(html.contains("Visible public update"));
    assert!(html.contains("&lt;private-by-default&gt;"));
    assert!(html.contains("/users/social/outbox"));
}

#[test]
fn parses_workers_ai_json_reply() {
    let parsed = parse_workers_ai_moderation(
        r#"{"unsafe":true,"categories":["Medical","sexual","work"],"summary":"contains private medical and work details"}"#,
    )
    .expect("expected JSON advisory");
    assert!(parsed.unsafe_detected);
    assert_eq!(
        normalize_ai_categories(parsed.categories),
        vec![
            "medical".to_string(),
            "adult".to_string(),
            "work-sensitive".to_string()
        ]
    );
    assert_eq!(
        parsed.summary.as_deref(),
        Some("contains private medical and work details")
    );
}

#[test]
fn normalizes_e2ee_device_ids_and_protocols() {
    assert_eq!(
        normalize_e2ee_device_id(" laptop:2026 ").unwrap(),
        "laptop:2026"
    );
    assert!(normalize_e2ee_device_id("bad device").is_err());
    assert!(normalize_e2ee_protocol("encryptedMessage-v1").is_err());
    assert_eq!(normalize_e2ee_protocol("OpenMLS").unwrap(), "mls-rfc9420");
    assert_eq!(
        normalize_e2ee_protocol("dais-mls-v2").unwrap(),
        "mls-rfc9420"
    );
    assert!(normalize_e2ee_protocol("legacy-rsa").is_err());
}

#[test]
fn validates_mls_device_material_shape() {
    let credential = BASE64.encode(b"https://social.dais.social/users/social#mac");
    let key_package = BASE64.encode(b"serialized-openmls-key-package");

    assert!(validate_e2ee_device_material("mls-rfc9420", &credential, &key_package).is_ok());
    assert!(validate_e2ee_device_material("mls-rfc9420", "not base64", &key_package).is_err());
    assert!(validate_e2ee_device_material("mls-rfc9420", &credential, "").is_err());
    assert!(normalize_e2ee_protocol("dais-mls-v1").is_err());
}

#[test]
fn parses_scoped_owner_tokens_from_secret_json_shapes() {
    let object_tokens = parse_scoped_owner_tokens(
        r#"{
            "release-smoke-token": "owner",
            "read-only-token": ["read", "profile"]
        }"#,
    );
    assert_eq!(object_tokens.len(), 2);
    assert!(object_tokens
        .iter()
        .any(|token| token.token == "release-smoke-token"
            && owner_token_has_scopes(&token.scopes, &["owner"])));
    assert!(object_tokens
        .iter()
        .any(|token| token.token == "read-only-token"
            && owner_token_has_scopes(&token.scopes, &["read", "profile"])
            && !owner_token_has_scopes(&token.scopes, &["write"])));

    let array_tokens = parse_scoped_owner_tokens(
        r#"[
            { "token": "write-token", "scopes": "write media" },
            { "value": "admin-token", "scope": "*" }
        ]"#,
    );
    assert_eq!(array_tokens.len(), 2);
    assert!(array_tokens
        .iter()
        .any(|token| token.token == "write-token"
            && owner_token_has_scopes(&token.scopes, &["write"])));
    assert!(array_tokens
        .iter()
        .any(|token| token.token == "admin-token"
            && owner_token_has_scopes(&token.scopes, &["owner"])));
}

#[test]
fn changed_peer_material_requires_explicit_retrust() {
    assert_eq!(
        peer_trust_state_after_material_update(Some("old"), Some("trusted"), "untrusted", "new"),
        "untrusted"
    );
    assert_eq!(
        peer_trust_state_after_material_update(Some("old"), Some("untrusted"), "trusted", "new"),
        "trusted"
    );
    assert_eq!(
        peer_trust_state_after_material_update(Some("same"), Some("trusted"), "untrusted", "same"),
        "trusted"
    );
    assert_eq!(
        peer_trust_state_after_material_update(Some("same"), Some("trusted"), "trusted", "same"),
        "trusted"
    );
    assert_eq!(
        peer_trust_state_after_material_update(Some("same"), Some("trusted"), "revoked", "same"),
        "revoked"
    );
}

#[test]
fn validates_dais_encrypted_message_v2_shape() {
    let envelope = serde_json::json!({
        "v": 2,
        "protocol": "mls-rfc9420",
        "groupId": "bWxzLWdyb3Vw",
        "epoch": 7,
        "senderDeviceId": "mac:2026",
        "ciphertext": BASE64.encode(b"serialized mls private message")
    });

    assert!(validate_dais_encrypted_message_v2(&envelope).is_ok());
    assert_eq!(
        validate_owner_e2ee_payload(&envelope).unwrap(),
        ("daisEncryptedMessage", "mls-rfc9420")
    );

    let mut bad_protocol = envelope.clone();
    bad_protocol["protocol"] = Value::String("dais-mls-v1".to_string());
    assert!(validate_dais_encrypted_message_v2(&bad_protocol).is_err());
    assert!(validate_owner_e2ee_payload(&bad_protocol).is_err());

    let mut bad_ciphertext = envelope.clone();
    bad_ciphertext["ciphertext"] = Value::String("not base64".to_string());
    assert!(validate_dais_encrypted_message_v2(&bad_ciphertext).is_err());

    let mut missing_epoch = envelope;
    missing_epoch.as_object_mut().unwrap().remove("epoch");
    assert!(validate_dais_encrypted_message_v2(&missing_epoch).is_err());
}

#[test]
fn owner_e2ee_payload_rejects_legacy_encrypted_message_v1() {
    let legacy = serde_json::json!({
        "v": 1,
        "alg": "AES-256-GCM",
        "keyWrap": "RSA-OAEP-256",
        "iv": "MDEyMzQ1Njc4OWFi",
        "ciphertext": "Y2lwaGVydGV4dA==",
        "recipients": [{
            "keyId": "peer-device",
            "wrappedKey": "d3JhcHBlZA=="
        }]
    });

    let error = validate_owner_e2ee_payload(&legacy).expect_err("legacy v1 must be rejected");
    assert!(error.contains("daisEncryptedMessage"));
}

#[test]
fn mls_activitypub_object_uses_dais_encrypted_message_without_v1_envelope() {
    let envelope = serde_json::json!({
        "v": 2,
        "protocol": "mls-rfc9420",
        "groupId": "bWxzLWdyb3Vw",
        "epoch": 1,
        "senderDeviceId": "dais-mac",
        "ciphertext": BASE64.encode(b"mls ciphertext")
    });
    let note = serde_json::json!({
        "type": "Note",
        "content": "Encrypted message. Open in a dais client to decrypt.",
        "daisE2ee": {
            "v": 2,
            "protocol": "mls-rfc9420",
            "senderDeviceId": "dais-mac",
            "groupId": envelope["groupId"].clone(),
            "epoch": envelope["epoch"].clone()
        },
        "daisEncryptedMessage": envelope
    });

    assert!(note.get("encryptedMessage").is_none());
    assert!(validate_dais_encrypted_message_v2(&note["daisEncryptedMessage"]).is_ok());
    assert_eq!(
        note["content"],
        "Encrypted message. Open in a dais client to decrypt."
    );
}

#[test]
fn computes_stable_e2ee_fingerprint() {
    let fingerprint = e2ee_device_fingerprint("credential", "key-package");
    assert_eq!(fingerprint.len(), 64);
    assert_eq!(
        normalize_e2ee_fingerprint(&format!("sha256:{fingerprint}")).unwrap(),
        fingerprint
    );
    assert!(normalize_e2ee_fingerprint("not-a-digest").is_err());
}

#[test]
fn media_metadata_records_storage_and_retention_fields() {
    let bytes = b"private media";
    let metadata = media_custom_metadata(MediaMetadataInput {
        owner: "https://social.dais.social/users/social",
        access: "private",
        media_type: "image/png",
        bytes,
        created_at: "2026-06-26T08:00:00.000Z",
        description: Some("alt text"),
        expires_at: Some("2026-06-27T08:00:00.000Z"),
        require_authorized_fetch: true,
    });

    assert_eq!(
        metadata.get("owner").map(String::as_str),
        Some("https://social.dais.social/users/social")
    );
    assert_eq!(
        metadata.get("visibility").map(String::as_str),
        Some("private")
    );
    assert_eq!(
        metadata.get("media_type").map(String::as_str),
        Some("image/png")
    );
    assert_eq!(metadata.get("size").map(String::as_str), Some("13"));
    let expected_hash = sha256_hex(bytes);
    assert_eq!(
        metadata.get("sha256").map(String::as_str),
        Some(expected_hash.as_str())
    );
    assert_eq!(
        metadata.get("created_at").map(String::as_str),
        Some("2026-06-26T08:00:00.000Z")
    );
    assert_eq!(
        metadata.get("description").map(String::as_str),
        Some("alt text")
    );
    assert_eq!(
        metadata.get("expires_at").map(String::as_str),
        Some("2026-06-27T08:00:00.000Z")
    );
    assert_eq!(
        metadata.get("authorized_fetch").map(String::as_str),
        Some("required")
    );
}

#[test]
fn validates_encrypted_media_payload_shape() {
    let payload = serde_json::json!({
        "v": 1,
        "alg": "AES-256-GCM",
        "iv": BASE64.encode([7u8; 12]),
        "ciphertext": BASE64.encode(b"ciphertext"),
        "mediaType": "image/png",
        "name": "secret.png"
    });

    assert!(validate_encrypted_media_payload(&payload).is_ok());

    let mut bad_iv = payload.clone();
    bad_iv["iv"] = Value::String(BASE64.encode([1u8; 8]));
    assert!(validate_encrypted_media_payload(&bad_iv).is_err());

    let mut bad_alg = payload;
    bad_alg["alg"] = Value::String("AES-128-GCM".to_string());
    assert!(validate_encrypted_media_payload(&bad_alg).is_err());
}

#[test]
fn normalizes_only_ciphertext_encrypted_media_attachments() {
    let encrypted = serde_json::json!({
        "type": "Document",
        "mediaType": "image/png",
        "name": "secret.png",
        "encryptedMedia": {
            "v": 1,
            "alg": "AES-256-GCM",
            "iv": BASE64.encode([9u8; 12]),
            "ciphertext": BASE64.encode(b"ciphertext"),
            "mediaType": "image/png",
            "name": "secret.png"
        }
    });
    let normalized = normalize_encrypted_media_attachments(&[encrypted]).unwrap();

    assert_eq!(normalized.len(), 1);
    let object = normalized[0].as_object().unwrap();
    assert!(object.get("encryptedMedia").is_some());
    assert!(object.get("url").is_none());
    assert!(object.get("data_base64").is_none());

    let plaintext = serde_json::json!({
        "type": "Document",
        "mediaType": "image/png",
        "name": "secret.png",
        "data_base64": BASE64.encode(b"secret image bytes")
    });
    assert!(normalize_encrypted_media_attachments(&[plaintext]).is_err());
}

#[test]
fn owner_post_media_must_be_private_for_followers_and_direct() {
    let public_url = serde_json::json!({
        "type": "Document",
        "mediaType": "image/png",
        "url": "https://social.dais.social/media/uploads/public.png"
    });
    let private_url = serde_json::json!({
        "type": "Document",
        "mediaType": "image/png",
        "url": "https://social.dais.social/media/_private/token/private.png"
    });

    for visibility in ["followers", "direct"] {
        let error =
            normalize_owner_post_attachments(&[public_url.clone()], "activitypub", visibility)
                .expect_err("public media must not attach to a non-public post");
        assert!(error.contains("private media upload URLs"));

        normalize_owner_post_attachments(&[private_url.clone()], "activitypub", visibility)
            .expect("private media should attach to a non-public post");
    }

    normalize_owner_post_attachments(&[public_url], "activitypub", "public")
        .expect("public media should attach to a public post");
}

#[test]
fn owner_post_media_for_atproto_must_be_a_public_image_upload() {
    let private_url = serde_json::json!({
        "type": "Document",
        "mediaType": "image/png",
        "url": "https://social.dais.social/media/_private/token/private.png"
    });

    let error = normalize_owner_post_attachments(&[private_url], "both", "public")
        .expect_err("private media must not be routed to AT Protocol");
    assert!(error.contains("public image uploads"));
}

#[test]
fn extracts_encrypted_media_attachments_from_activitypub_object() {
    let note = serde_json::json!({
        "type": "Note",
        "attachment": [{
            "type": "Document",
            "mediaType": "image/png",
            "name": "secret.png",
            "encryptedMedia": {
                "v": 1,
                "alg": "AES-256-GCM",
                "iv": BASE64.encode([3u8; 12]),
                "ciphertext": BASE64.encode(b"ciphertext"),
                "mediaType": "image/png",
                "name": "secret.png"
            }
        }]
    });

    let attachments = encrypted_media_attachments_from_activitypub_object(&note).unwrap();

    assert_eq!(attachments.len(), 1);
    assert_eq!(
        attachments[0]["name"],
        Value::String("secret.png".to_string())
    );
    assert!(attachments[0].get("encryptedMedia").is_some());
}

#[test]
fn atproto_media_validation_allows_public_images_only() {
    let public_image = serde_json::json!({
        "type": "Image",
        "url": "https://social.dais.social/media/uploads/photo.png",
        "mediaType": "image/png",
        "name": "public photo"
    });
    let private_image = serde_json::json!({
        "type": "Image",
        "url": "https://social.dais.social/media/_private/token/photo.png",
        "mediaType": "image/png"
    });
    let public_video = serde_json::json!({
        "type": "Document",
        "url": "https://social.dais.social/media/uploads/video.mp4",
        "mediaType": "video/mp4"
    });

    assert!(is_public_atproto_image_attachment(&public_image));
    assert!(!is_public_atproto_image_attachment(&private_image));
    assert!(!is_public_atproto_image_attachment(&public_video));
}

#[test]
fn display_local_url_preserves_remote_reply_targets() {
    assert_eq!(
        display_local_url(
            "https://social.skpt.cl",
            "https://social.dais.social/users/social/posts/abc"
        ),
        "https://social.dais.social/users/social/posts/abc"
    );
    assert_eq!(
        display_local_url(
            "https://social.skpt.cl",
            "https://social.skpt.cl/users/social/posts/abc"
        ),
        "https://social.skpt.cl/users/social/posts/abc"
    );
}

#[test]
fn local_object_detection_uses_current_instance_host() {
    assert!(!is_local_object_url(
        "https://social.dais.social/users/social/posts/abc",
        "social.skpt.cl"
    ));
    assert!(is_local_object_url(
        "https://social.skpt.cl/users/social/posts/abc",
        "social.skpt.cl"
    ));
}

#[test]
fn strips_markdown_json_fence() {
    assert_eq!(
        strip_json_fence("```json\n{\"unsafe\":false}\n```"),
        "{\"unsafe\":false}"
    );
}

#[test]
fn maps_watch_kinds_to_explicit_source_types() {
    assert_eq!(source_type_for_watch_kind("rss"), Some("watch_rss"));
    assert_eq!(
        source_type_for_watch_kind("activitypub_actor"),
        Some("watch_activitypub_actor")
    );
    assert_eq!(
        source_type_for_watch_kind("bluesky_actor"),
        Some("watch_bluesky_actor")
    );
}

#[test]
fn private_watch_policy_forces_no_remote_relationship() {
    let body = serde_json::json!({
        "private_reader_only": false,
        "excerpt_only": true,
        "image_allowed": true,
        "full_text_allowed": true
    });

    let policy: Value =
        serde_json::from_str(&source_policy_json_for_type(&body, "watch_bluesky_actor")).unwrap();
    assert_eq!(policy.get("private_reader_only"), Some(&Value::Bool(true)));
    assert_eq!(policy.get("watch"), Some(&Value::Bool(true)));
    assert_eq!(policy.get("public_only"), Some(&Value::Bool(true)));
    assert_eq!(
        policy.get("no_remote_relationship"),
        Some(&Value::Bool(true))
    );

    let source_policy: Value =
        serde_json::from_str(&source_policy_json_for_type(&body, "rss")).unwrap();
    assert_eq!(source_policy.get("watch"), Some(&Value::Bool(false)));
    assert_eq!(
        source_policy.get("no_remote_relationship"),
        Some(&Value::Bool(false))
    );
}

#[test]
fn audience_group_metadata_defaults_private_and_distinguishes_purpose() {
    assert_eq!(normalize_audience_group_type("audience"), "audience");
    assert_eq!(
        normalize_audience_group_type("private-group"),
        "private_group"
    );
    assert_eq!(normalize_audience_group_type("community"), "private_group");

    assert_eq!(normalize_audience_membership_visibility(""), "private");
    assert_eq!(
        normalize_audience_membership_visibility("members"),
        "members"
    );
    assert_eq!(normalize_audience_membership_visibility("public"), "public");

    assert_eq!(normalize_audience_posting_policy(""), "owner");
    assert_eq!(normalize_audience_posting_policy("members"), "members");
    assert_eq!(
        audience_group_purpose_label("private_group"),
        "Private group"
    );
    assert_eq!(audience_membership_label("private"), "Membership private");
}

#[test]
fn normalizes_private_watch_targets_by_protocol() {
    let bluesky = serde_json::json!({
        "target": "https://bsky.app/profile/nasa.gov"
    });
    assert_eq!(
        normalized_source_target("watch_bluesky_actor", &bluesky).unwrap(),
        "nasa.gov"
    );

    let activitypub = serde_json::json!({
        "actor": "@alice@example.social"
    });
    assert_eq!(
        normalized_source_target("watch_activitypub_actor", &activitypub).unwrap(),
        "@alice@example.social"
    );

    let feed = serde_json::json!({
        "url": "https://example.com/feed.xml"
    });
    assert_eq!(
        normalized_source_target("watch_rss", &feed).unwrap(),
        "https://example.com/feed.xml"
    );

    let insecure = serde_json::json!({
        "url": "http://example.com/feed.xml"
    });
    assert_eq!(
        normalized_source_target("watch_rss", &insecure).unwrap_err(),
        "watch target must use https"
    );
}

#[test]
fn source_ids_dedupe_by_type_and_normalized_target() {
    let one = source_id("watch_bluesky_actor", "nasa.gov");
    let two = source_id("watch_bluesky_actor", "nasa.gov");
    let different_type = source_id("watch_bluesky_post", "nasa.gov");

    assert_eq!(one, two);
    assert_ne!(one, different_type);
    assert!(one.starts_with("source-"));
}

#[test]
fn normalizes_bluesky_watch_targets() {
    assert_eq!(
        bluesky_actor_target("https://bsky.app/profile/nasa.gov").unwrap(),
        "nasa.gov"
    );
    assert_eq!(bluesky_actor_target("@nasa.gov").unwrap(), "nasa.gov");
    assert_eq!(
        bluesky_post_uri("https://bsky.app/profile/nasa.gov/post/3abc").unwrap(),
        "at://nasa.gov/app.bsky.feed.post/3abc"
    );
}

#[test]
fn parses_public_search_filters() {
    let url = worker::Url::parse(
            "https://social.dais.social/api/dais/owner/search?q=launch&scope=public&provider=activitypub&type=posts&server=mastodon.social,fosstodon.org&sort=top&tag=space&tag=%23science&lang=en",
        )
        .unwrap();
    let options = OwnerPublicSearchOptions::from_url(&url);
    assert_eq!(options.provider, OwnerPublicSearchProvider::ActivityPub);
    assert_eq!(options.result_type, OwnerPublicSearchResultType::Posts);
    assert_eq!(
        options.activitypub_servers,
        vec!["mastodon.social".to_string(), "fosstodon.org".to_string()]
    );
    assert_eq!(options.filters.sort.as_deref(), Some("top"));
    assert_eq!(options.filters.lang.as_deref(), Some("en"));
    assert_eq!(
        options.filters.tags,
        vec!["space".to_string(), "science".to_string()]
    );
}

#[test]
fn parses_tootfinder_public_search_provider() {
    let url = worker::Url::parse(
            "https://social.dais.social/api/dais/owner/search?q=science&scope=public&provider=tootfinder&type=posts",
        )
        .unwrap();
    let options = OwnerPublicSearchOptions::from_url(&url);
    assert_eq!(options.provider, OwnerPublicSearchProvider::Tootfinder);
    assert!(options.includes_tootfinder());
    assert!(options.includes_posts());
    assert!(!options.includes_activitypub());
}

#[test]
fn bluesky_public_search_uses_appview_host() {
    assert_eq!(
        bluesky_appview_xrpc_url("app.bsky.feed.searchPosts", "q=science&limit=3"),
        "https://api.bsky.app/xrpc/app.bsky.feed.searchPosts?q=science&limit=3"
    );
    assert_eq!(
        bluesky_appview_xrpc_url(
            "app.bsky.feed.getAuthorFeed",
            "actor=nasa.gov&limit=50&filter=posts_no_replies"
        ),
        "https://api.bsky.app/xrpc/app.bsky.feed.getAuthorFeed?actor=nasa.gov&limit=50&filter=posts_no_replies"
    );
}

#[test]
fn mastodon_public_search_does_not_request_authenticated_resolution() {
    let params = owner_public_search_mastodon_query_params(
        "science",
        3,
        &OwnerPublicSearchResultType::Actors,
    );
    assert!(params
        .iter()
        .all(|(key, _)| key != "resolve" && key != "resolve[]"));
    assert!(params
        .iter()
        .any(|(key, value)| key == "type" && value == "accounts"));
}

#[test]
fn tootfinder_public_search_url_encodes_path_query() {
    assert_eq!(
        tootfinder_search_url("san OR francisco"),
        "https://www.tootfinder.ch/rest/api/search/san%20OR%20francisco"
    );
}

#[test]
fn parses_tootfinder_json_after_warning_prelude() {
    let body = r#"<br />
<b>Warning</b>: Undefined array key<br />
[{"id":"1","uri":"https://example.social/users/alice/statuses/1","url":"https://example.social/@alice/1","content":"<p>Hello</p>"}]"#;
    let parsed = parse_lenient_json_body(body).expect("tootfinder JSON");
    assert_eq!(tootfinder_search_items(&parsed).len(), 1);
}

#[test]
fn normalizes_public_search_actions() {
    let post = serde_json::json!({
        "uri": "at://did:plc:alice/app.bsky.feed.post/3abc",
        "cid": "bafy",
        "author": {
            "did": "did:plc:alice",
            "handle": "alice.example",
            "displayName": "Alice"
        },
        "record": {
            "$type": "app.bsky.feed.post",
            "text": "A searchable public Bluesky update",
            "createdAt": "2026-06-18T12:00:00Z"
        }
    });
    let row = owner_normalize_bluesky_post(&post).expect("public search post");
    assert_eq!(
        row.get("watch_type").and_then(Value::as_str),
        Some("bluesky_post")
    );
    assert_eq!(
        row.get("reply_target").and_then(Value::as_str),
        Some("at://did:plc:alice/app.bsky.feed.post/3abc")
    );

    let discovered = normalize_discovered_public_post(&serde_json::json!({
        "type": "Create",
        "actor": "https://example.com/users/alice",
        "to": [PUBLIC_COLLECTION],
        "object": {
            "type": "Note",
            "id": "https://example.com/posts/1",
            "attributedTo": "https://example.com/users/alice",
            "to": [PUBLIC_COLLECTION],
            "content": "<p>Hello public world</p>",
            "published": "2026-06-18T12:00:00Z",
            "url": "https://example.com/@alice/1"
        }
    }))
    .expect("activitypub post");
    let row = owner_public_post_row_from_discovered("direct", &discovered);
    assert_eq!(
        row.get("watch_type").and_then(Value::as_str),
        Some("activitypub_object")
    );
    assert_eq!(
        row.get("reply_target").and_then(Value::as_str),
        Some("https://example.com/posts/1")
    );

    let row = owner_normalize_tootfinder_status(serde_json::json!({
        "id": "116430909593124640",
        "created_at": "2026-04-19 10:31:29",
        "spoiler": "",
        "visibility": "public",
        "language": "en",
        "uri": "https://mastodon.social/users/ubuntourist/statuses/116430909593124640",
        "url": "https://mastodon.social/@ubuntourist/116430909593124640",
        "content": "<p>Searchable public science post</p>"
    }))
    .expect("tootfinder public search post");
    assert_eq!(
        row.get("provider").and_then(Value::as_str),
        Some("tootfinder.ch")
    );
    assert_eq!(
        row.get("actor_id").and_then(Value::as_str),
        Some("https://mastodon.social/users/ubuntourist")
    );
    assert_eq!(
        row.get("actor_handle").and_then(Value::as_str),
        Some("ubuntourist@mastodon.social")
    );
    assert_eq!(
        row.get("watch_type").and_then(Value::as_str),
        Some("activitypub_object")
    );
    assert_eq!(
        row.get("reply_target").and_then(Value::as_str),
        Some("https://mastodon.social/users/ubuntourist/statuses/116430909593124640")
    );
}

#[test]
fn normalizes_activitypub_public_watch_item() {
    let activity = serde_json::json!({
        "type": "Create",
        "id": "https://example.com/create/1",
        "actor": "https://example.com/users/alice",
        "to": [PUBLIC_COLLECTION],
        "object": {
            "type": "Note",
            "id": "https://example.com/posts/1",
            "attributedTo": "https://example.com/users/alice",
            "to": [PUBLIC_COLLECTION],
            "content": "<p>Hello public world</p>",
            "published": "2026-06-18T12:00:00Z",
            "url": "https://example.com/@alice/1"
        }
    });
    let post = normalize_discovered_public_post(&activity).expect("public post");
    let mut source = Map::new();
    source.insert("id".to_string(), Value::String("source-test".to_string()));
    let policy = SourcePolicy::default();
    let item = activitypub_watch_item(&source, &post, &policy).expect("watch item");
    assert_eq!(
        item.external_id.as_deref(),
        Some("https://example.com/posts/1")
    );
    assert_eq!(
        item.author.as_deref(),
        Some("https://example.com/users/alice")
    );
    assert_eq!(item.excerpt.as_deref(), Some("Hello public world"));
}

#[test]
fn normalizes_bluesky_public_watch_item() {
    let post = serde_json::json!({
        "uri": "at://did:plc:alice/app.bsky.feed.post/3abc",
        "cid": "bafy",
        "author": {
            "did": "did:plc:alice",
            "handle": "alice.example",
            "displayName": "Alice"
        },
        "record": {
            "$type": "app.bsky.feed.post",
            "text": "A public Bluesky update",
            "createdAt": "2026-06-18T12:00:00Z"
        }
    });
    let mut source = Map::new();
    source.insert("id".to_string(), Value::String("source-bsky".to_string()));
    let policy = SourcePolicy::default();
    let item = bluesky_watch_item(&source, &post, &policy).expect("watch item");
    assert_eq!(
        item.canonical_url.as_deref(),
        Some("https://bsky.app/profile/alice.example/post/3abc")
    );
    assert_eq!(item.author.as_deref(), Some("Alice"));
    assert_eq!(item.excerpt.as_deref(), Some("A public Bluesky update"));
}
