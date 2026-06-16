use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use worker::{event, Context, D1Type, Env, Headers, Request, Response, Result};

const PUBLIC_COLLECTION: &str = "https://www.w3.org/ns/activitystreams#Public";

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;
    let path = url.path();
    let host = url.host_str().unwrap_or_default();

    if host == "social.dais.social" && path == "/" {
        let target = url.join("/users/social")?;
        return Response::redirect(target);
    }

    if path.starts_with("/api/dais/owner/") {
        return handle_owner_api(req, env, &url).await;
    }

    match path {
        "/__dais-fixtures/activitypub/actor" => fixture_actor_response(&url),
        "/__dais-fixtures/activitypub/outbox" => fixture_outbox_response(&url),
        "/__dais-fixtures/activitypub/posts/public-preview" => fixture_post_response(&url),
        "/health" => Response::ok("OK"),
        _ => Response::error(
            "Rust router migration scaffold: route not migrated yet",
            501,
        ),
    }
}

async fn handle_owner_api(mut req: Request, env: Env, url: &worker::Url) -> Result<Response> {
    if req.method() == worker::Method::Options {
        return api_json(&serde_json::json!({}), 204);
    }

    let path = url.path();
    let owner_path = path
        .strip_prefix("/api/dais/owner")
        .filter(|value| !value.is_empty())
        .unwrap_or("/");
    let limit = clamp_limit(query_param(url, "limit"));
    if let Some(response) = require_owner_bearer(
        &req,
        &env,
        owner_api_required_scopes(req.method(), owner_path),
    )? {
        return Ok(response);
    }

    match (req.method(), owner_path) {
        (worker::Method::Get, "/snapshot") => api_json(&owner_snapshot(&env).await?, 200),
        (worker::Method::Get, "/profile") => api_json(&owner_profile(&env).await?, 200),
        (worker::Method::Get, "/stats") => api_json(&owner_stats(&env).await?, 200),
        (worker::Method::Get, "/diagnostics") => api_json(
            &serde_json::json!({ "items": owner_diagnostics(&env).await? }),
            200,
        ),
        (worker::Method::Get, "/followers") => api_json(
            &OwnerItems {
                items: owner_followers(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Get, "/friends") => api_json(
            &OwnerItems {
                items: owner_friends(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Get, "/following") => api_json(
            &OwnerItems {
                items: owner_following(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Get, "/posts") => api_json(
            &OwnerItems {
                items: owner_posts(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Get, _) if owner_path.starts_with("/posts/") => {
            let post_id = decode_component(owner_path.trim_start_matches("/posts/"));
            match owner_post_detail(&env, &post_id).await? {
                Some(post) => api_json(&post, 200),
                None => api_json(&serde_json::json!({ "error": "post not found" }), 404),
            }
        }
        (worker::Method::Get, "/timeline/home") => api_json(
            &OwnerItems {
                items: owner_home_timeline(
                    &env,
                    limit,
                    query_param(url, "include_replies").as_deref() == Some("true"),
                )
                .await?,
            },
            200,
        ),
        (worker::Method::Get, "/notifications") => api_json(
            &OwnerItems {
                items: owner_notifications(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Post, "/notifications/read") => {
            let body = read_json(&mut req).await;
            let Some(id) = required_body_string(body.get("id")) else {
                return api_json(&serde_json::json!({ "error": "id is required" }), 400);
            };
            owner_mark_notification_read(&env, &id).await?;
            api_json(&serde_json::json!({ "ok": true }), 200)
        }
        (worker::Method::Get, "/deliveries") => api_json(
            &OwnerItems {
                items: owner_deliveries(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Get, "/direct-messages") => api_json(
            &OwnerItems {
                items: owner_direct_messages(&env, limit).await?,
            },
            200,
        ),
        (worker::Method::Get, "/search") => api_json(
            &owner_search(&env, query_param(url, "q").unwrap_or_default(), limit).await?,
            200,
        ),
        (worker::Method::Get, "/sources") => api_json(
            &OwnerSources {
                subscriptions: owner_source_subscriptions(&env, limit).await?,
                items: owner_source_items(
                    &env,
                    clamp_limit(query_param(url, "items_limit").or_else(|| Some("40".to_string()))),
                )
                .await?,
            },
            200,
        ),
        (worker::Method::Delete, _) if owner_path.starts_with("/sources/") => {
            let id = decode_component(owner_path.trim_start_matches("/sources/"));
            if id.trim().is_empty() {
                return api_json(&serde_json::json!({ "error": "id is required" }), 400);
            }
            owner_delete_source(&env, &id).await?;
            api_json(&serde_json::json!({ "ok": true }), 200)
        }
        (worker::Method::Get, "/moderation") => api_json(&owner_moderation(&env).await?, 200),
        _ => api_json(
            &serde_json::json!({ "error": "Rust router migration scaffold: owner route not migrated yet" }),
            501,
        ),
    }
}

fn owner_api_required_scopes(method: worker::Method, path: &str) -> &'static [&'static str] {
    match method {
        worker::Method::Get => &["read"],
        worker::Method::Delete => &["write"],
        _ if path == "/discovery/actor" => &["read"],
        _ if path == "/followers/status"
            || path == "/following/follow"
            || path == "/following/unfollow" =>
        {
            &["follow"]
        }
        _ if path.starts_with("/moderation/") => &["moderation"],
        _ if path == "/media" || path == "/media/revoke" => &["media"],
        _ => &["write"],
    }
}

async fn owner_profile(env: &Env) -> Result<OwnerProfile> {
    let db = env.d1("DB")?;
    let row = db
        .prepare(
            r#"
            SELECT id, username, COALESCE(actor_type, 'Person') AS actor_type,
                   display_name, summary, icon, image, avatar_url, header_url
            FROM actors
            WHERE username = 'social'
            LIMIT 1
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await?;
    let username = string_field(row.as_ref(), "username").unwrap_or_else(|| "social".to_string());
    let actor_url = string_field(row.as_ref(), "id")
        .unwrap_or_else(|| "https://social.dais.social/users/social".to_string());
    let actor_type =
        string_field(row.as_ref(), "actor_type").unwrap_or_else(|| "Person".to_string());
    let handle_domain = env
        .var("DOMAIN")
        .map(|value| value.to_string())
        .unwrap_or_else(|_| "dais.social".to_string());
    let icon = string_field(row.as_ref(), "icon");
    let image = string_field(row.as_ref(), "image");
    Ok(OwnerProfile {
        id: actor_url.clone(),
        username: username.clone(),
        actor_type,
        display_name: string_field(row.as_ref(), "display_name"),
        summary: string_field(row.as_ref(), "summary"),
        avatar_url: string_field(row.as_ref(), "avatar_url").or_else(|| icon.clone()),
        header_url: string_field(row.as_ref(), "header_url").or_else(|| image.clone()),
        icon,
        image,
        public_handle: format!("@{username}@{handle_domain}"),
        actor_url,
    })
}

async fn owner_stats(env: &Env) -> Result<OwnerStats> {
    let db = env.d1("DB")?;
    let row = db
        .prepare(
            r#"
            SELECT
                (SELECT COUNT(*) FROM followers) AS followers_total,
                (SELECT COUNT(*) FROM followers WHERE status='approved') AS followers_approved,
                (SELECT COUNT(*) FROM followers WHERE status='pending') AS followers_pending,
                (SELECT COUNT(*) FROM followers WHERE status='rejected') AS followers_rejected,
                (SELECT COUNT(*) FROM following) AS following_total,
                (SELECT COUNT(*) FROM posts) AS posts_total,
                (SELECT COUNT(*) FROM activities) AS activities_total,
                (SELECT COUNT(*) FROM deliveries) AS deliveries_total,
                (SELECT COUNT(*) FROM deliveries WHERE status='failed') AS deliveries_failed,
                (SELECT COUNT(*) FROM deliveries WHERE status='queued') AS deliveries_queued,
                (SELECT COUNT(*) FROM deliveries WHERE status='retry') AS deliveries_retry,
                (SELECT COUNT(*) FROM deliveries WHERE status='delivered') AS deliveries_delivered,
                (SELECT COUNT(*) FROM posts WHERE protocol='both') AS dual_protocol_posts,
                (SELECT COUNT(*) FROM posts WHERE visibility='public') AS public_posts,
                (SELECT COUNT(*) FROM posts WHERE visibility IN ('followers', 'unlisted')) AS private_posts,
                (SELECT COUNT(*) FROM posts WHERE visibility='direct') AS direct_posts,
                (SELECT COUNT(*) FROM posts WHERE encrypted_message IS NOT NULL) AS encrypted_posts,
                (SELECT COUNT(*) FROM posts WHERE media_attachments IS NOT NULL AND media_attachments != '') AS media_posts,
                (SELECT COUNT(*) FROM notifications WHERE read = 0 OR read IS NULL) AS notifications_unread,
                (SELECT COUNT(*) FROM blocks) AS blocks_total,
                (SELECT COUNT(*) FROM federation_allowlist WHERE enabled = 1) AS allowlist_hosts,
                (SELECT closed_network FROM instance_settings WHERE id = 1) AS closed_network
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await?;
    Ok(OwnerStats {
        followers_total: integer_field(row.as_ref(), "followers_total"),
        followers_approved: integer_field(row.as_ref(), "followers_approved"),
        followers_pending: integer_field(row.as_ref(), "followers_pending"),
        followers_rejected: integer_field(row.as_ref(), "followers_rejected"),
        following_total: integer_field(row.as_ref(), "following_total"),
        posts_total: integer_field(row.as_ref(), "posts_total"),
        activities_total: integer_field(row.as_ref(), "activities_total"),
        deliveries_total: integer_field(row.as_ref(), "deliveries_total"),
        deliveries_failed: integer_field(row.as_ref(), "deliveries_failed"),
        deliveries_queued: integer_field(row.as_ref(), "deliveries_queued"),
        deliveries_retry: integer_field(row.as_ref(), "deliveries_retry"),
        deliveries_delivered: integer_field(row.as_ref(), "deliveries_delivered"),
        dual_protocol_posts: integer_field(row.as_ref(), "dual_protocol_posts"),
        public_posts: integer_field(row.as_ref(), "public_posts"),
        private_posts: integer_field(row.as_ref(), "private_posts"),
        direct_posts: integer_field(row.as_ref(), "direct_posts"),
        encrypted_posts: integer_field(row.as_ref(), "encrypted_posts"),
        media_posts: integer_field(row.as_ref(), "media_posts"),
        notifications_unread: integer_field(row.as_ref(), "notifications_unread"),
        blocks_total: integer_field(row.as_ref(), "blocks_total"),
        allowlist_hosts: integer_field(row.as_ref(), "allowlist_hosts"),
        closed_network: integer_field(row.as_ref(), "closed_network") != 0,
    })
}

async fn owner_diagnostics(env: &Env) -> Result<Vec<OwnerDiagnostic>> {
    let db = env.d1("DB")?;
    let settings = db
        .prepare(
            r#"
            SELECT default_visibility
            FROM instance_settings
            WHERE id = 1
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await?;
    let posts = db
        .prepare("SELECT COUNT(*) AS count FROM posts")
        .first::<Map<String, Value>>(None)
        .await?;
    let followers = db
        .prepare("SELECT COUNT(*) AS count FROM followers WHERE status = 'approved'")
        .first::<Map<String, Value>>(None)
        .await?;
    let deliveries = db
        .prepare("SELECT status, COUNT(*) AS count FROM deliveries GROUP BY status")
        .all()
        .await?
        .results::<DeliveryCount>()?;
    let default_visibility = string_field(settings.as_ref(), "default_visibility")
        .unwrap_or_else(|| "followers".to_string());
    let failed_deliveries = deliveries
        .iter()
        .find(|row| row.status == "failed")
        .map(|row| row.count)
        .unwrap_or(0);
    let delivery_detail = if deliveries.is_empty() {
        "no deliveries".to_string()
    } else {
        deliveries
            .iter()
            .map(|row| format!("{}={}", row.status, row.count))
            .collect::<Vec<_>>()
            .join(" ")
    };
    Ok(vec![
        OwnerDiagnostic {
            key: "owner-api",
            ok: true,
            detail: "Authenticated owner API is available.".to_string(),
        },
        OwnerDiagnostic {
            key: "private-default",
            ok: default_visibility == "followers",
            detail: format!("default visibility is {default_visibility}"),
        },
        OwnerDiagnostic {
            key: "activitypub",
            ok: true,
            detail: format!(
                "posts={} approved_followers={}",
                integer_field(posts.as_ref(), "count"),
                integer_field(followers.as_ref(), "count")
            ),
        },
        OwnerDiagnostic {
            key: "deliveries",
            ok: failed_deliveries == 0,
            detail: delivery_detail,
        },
    ])
}

async fn owner_settings(env: &Env) -> Result<Map<String, Value>> {
    let db = env.d1("DB")?;
    Ok(db
        .prepare(
            r#"
            SELECT default_visibility, require_authorized_fetch, manually_approves_followers,
                   COALESCE(closed_network, 0) AS closed_network
            FROM instance_settings
            WHERE id = 1
            "#,
        )
        .first::<Map<String, Value>>(None)
        .await?
        .unwrap_or_else(|| {
            let mut settings = Map::new();
            settings.insert(
                "default_visibility".to_string(),
                Value::String("followers".to_string()),
            );
            settings.insert("require_authorized_fetch".to_string(), Value::from(1));
            settings.insert("manually_approves_followers".to_string(), Value::from(1));
            settings.insert("closed_network".to_string(), Value::from(0));
            settings
        }))
}

async fn owner_snapshot(env: &Env) -> Result<Map<String, Value>> {
    let profile = owner_profile(env).await?;
    let home_timeline = owner_home_timeline(env, 20, false).await?;
    let posts = owner_posts(env, 20).await?;
    let followers = owner_followers(env, 100).await?;
    let friends = owner_friends(env, 100).await?;
    let following = owner_following(env, 100).await?;
    let sources = owner_source_items(env, 20).await?;
    let moderation = owner_moderation(env).await?;
    let diagnostics = owner_diagnostics(env).await?;
    let settings = owner_settings(env).await?;

    let mut snapshot_settings = Map::new();
    snapshot_settings.insert(
        "instance_url".to_string(),
        Value::String("https://social.dais.social".to_string()),
    );
    snapshot_settings.insert("owner_token_present".to_string(), Value::Bool(true));
    let default_visibility = string_field(Some(&settings), "default_visibility")
        .unwrap_or_else(|| "followers".to_string());
    snapshot_settings.insert(
        "default_visibility".to_string(),
        Value::String(title_visibility(Some(default_visibility.as_str()))),
    );
    snapshot_settings.insert(
        "default_protocol".to_string(),
        Value::String("Both".to_string()),
    );

    let mut snapshot = Map::new();
    snapshot.insert("settings".to_string(), Value::Object(snapshot_settings));
    snapshot.insert(
        "active_section".to_string(),
        Value::String("Home".to_string()),
    );
    snapshot.insert("profile".to_string(), serde_json::json!(profile));
    snapshot.insert(
        "home_timeline".to_string(),
        Value::Array(
            home_timeline
                .into_iter()
                .map(shape_snapshot_home_timeline_item)
                .map(Value::Object)
                .collect(),
        ),
    );
    snapshot.insert(
        "posts".to_string(),
        Value::Array(
            posts
                .into_iter()
                .map(shape_snapshot_post)
                .map(Value::Object)
                .collect(),
        ),
    );
    snapshot.insert(
        "followers".to_string(),
        Value::Array(followers.into_iter().map(Value::Object).collect()),
    );
    snapshot.insert(
        "friends".to_string(),
        Value::Array(friends.into_iter().map(Value::Object).collect()),
    );
    snapshot.insert(
        "following".to_string(),
        Value::Array(following.into_iter().map(Value::Object).collect()),
    );
    snapshot.insert(
        "sources".to_string(),
        Value::Array(sources.into_iter().map(Value::Object).collect()),
    );
    snapshot.insert("moderation".to_string(), serde_json::json!(moderation));
    snapshot.insert("diagnostics".to_string(), serde_json::json!(diagnostics));
    Ok(snapshot)
}

fn shape_snapshot_home_timeline_item(post: Map<String, Value>) -> Map<String, Value> {
    let mut item = Map::new();
    item.insert("id".to_string(), row_value_or_null(&post, "id"));
    item.insert(
        "object_id".to_string(),
        row_value_or_null(&post, "object_id"),
    );
    item.insert("actor_id".to_string(), row_value_or_null(&post, "actor_id"));
    item.insert(
        "actor_username".to_string(),
        row_value_or_null(&post, "actor_username"),
    );
    item.insert(
        "actor_display_name".to_string(),
        row_value_or_null(&post, "actor_display_name"),
    );
    item.insert(
        "actor_avatar_url".to_string(),
        row_value_or_null(&post, "actor_avatar_url"),
    );
    item.insert(
        "content".to_string(),
        string_value_or_default(&post, "content"),
    );
    item.insert(
        "content_html".to_string(),
        row_value_or_null(&post, "content_html"),
    );
    item.insert(
        "visibility".to_string(),
        Value::String(
            string_field(Some(&post), "visibility").unwrap_or_else(|| "public".to_string()),
        ),
    );
    item.insert(
        "in_reply_to".to_string(),
        row_value_or_null(&post, "in_reply_to"),
    );
    item.insert(
        "published_at".to_string(),
        row_value_or_null(&post, "published_at"),
    );
    item.insert(
        "protocol".to_string(),
        Value::String(
            string_field(Some(&post), "protocol").unwrap_or_else(|| "activitypub".to_string()),
        ),
    );
    item.insert(
        "reply_count".to_string(),
        Value::from(integer_field(Some(&post), "reply_count")),
    );
    item.insert(
        "like_count".to_string(),
        Value::from(integer_field(Some(&post), "like_count")),
    );
    item.insert(
        "boost_count".to_string(),
        Value::from(integer_field(Some(&post), "boost_count")),
    );
    item
}

fn shape_snapshot_post(post: Map<String, Value>) -> Map<String, Value> {
    let mut item = Map::new();
    item.insert("id".to_string(), row_value_or_null(&post, "id"));
    item.insert("title".to_string(), row_value_or_null(&post, "name"));
    item.insert(
        "content".to_string(),
        string_value_or_default(&post, "content"),
    );
    item.insert(
        "visibility".to_string(),
        Value::String(title_visibility(
            string_field(Some(&post), "visibility").as_deref(),
        )),
    );
    item.insert(
        "protocol".to_string(),
        Value::String(title_protocol(
            string_field(Some(&post), "protocol").as_deref(),
        )),
    );
    item.insert(
        "encrypted".to_string(),
        Value::Bool(non_empty_value(&post, "encrypted_message").is_some()),
    );
    item.insert(
        "attachments".to_string(),
        Value::Array(parse_attachment_array(post.get("media_attachments"))),
    );
    item.insert(
        "reply_count".to_string(),
        Value::from(integer_field(Some(&post), "reply_count")),
    );
    item.insert(
        "like_count".to_string(),
        Value::from(integer_field(Some(&post), "like_count")),
    );
    item.insert(
        "boost_count".to_string(),
        Value::from(integer_field(Some(&post), "boost_count")),
    );
    item.insert(
        "published_at".to_string(),
        row_value_or_null(&post, "published_at"),
    );
    item
}

async fn owner_moderation(env: &Env) -> Result<OwnerModeration> {
    let db = env.d1("DB")?;
    let settings = owner_settings(env).await?;
    let blocks = db
        .prepare("SELECT COUNT(*) AS count FROM blocks")
        .first::<Map<String, Value>>(None)
        .await?;
    let allowlist = db
        .prepare("SELECT COUNT(*) AS count FROM federation_allowlist WHERE enabled = 1")
        .first::<Map<String, Value>>(None)
        .await?;
    Ok(OwnerModeration {
        closed_network: bool_field(Some(&settings), "closed_network"),
        block_count: integer_field(blocks.as_ref(), "count"),
        allowlist_count: integer_field(allowlist.as_ref(), "count"),
        require_authorized_fetch: bool_field(Some(&settings), "require_authorized_fetch"),
        manually_approves_followers: bool_field(Some(&settings), "manually_approves_followers"),
        blocks: owner_blocks(env).await?,
        allowlist: owner_allowlist(env).await?,
    })
}

async fn owner_blocks(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT id, actor_id, blocked_domain, reason, created_at
        FROM blocks
        ORDER BY created_at DESC
        LIMIT 80
        "#,
    )
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_allowlist(env: &Env) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    db.prepare(
        r#"
        SELECT host, note, enabled, created_at, updated_at
        FROM federation_allowlist
        ORDER BY host ASC
        LIMIT 120
        "#,
    )
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_followers(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, actor_id, follower_actor_id, follower_inbox, follower_shared_inbox,
               status, created_at, updated_at
        FROM followers
        ORDER BY
          CASE status WHEN 'pending' THEN 0 WHEN 'approved' THEN 1 ELSE 2 END,
          updated_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_friends(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let local_actor = owner_local_actor(env).await?;
    let actor_arg = D1Type::Text(&local_actor.id);
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT friend_actor_id, friend_inbox, friend_shared_inbox,
               follower_since, following_since, accepted_at
        FROM friends
        WHERE local_actor_id = ?1
        ORDER BY COALESCE(accepted_at, following_since, follower_since) DESC
        LIMIT ?2
        "#,
    )
    .bind_refs([&actor_arg, &limit_arg])?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_following(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, actor_id, target_actor_id, target_inbox, status, created_at, accepted_at
        FROM following
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_posts(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
               name, summary, visibility, COALESCE(protocol, 'activitypub') AS protocol,
               atproto_uri, atproto_cid, encrypted_message, media_attachments,
               published_at, created_at, updated_at,
               (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count
        FROM posts
        ORDER BY published_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_post_detail(env: &Env, id: &str) -> Result<Option<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    let post = db
        .prepare(
            r#"
            SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
                   name, summary, visibility, COALESCE(protocol, 'activitypub') AS protocol,
                   atproto_uri, atproto_cid, encrypted_message, media_attachments,
                   published_at, created_at, updated_at, in_reply_to,
                   (SELECT COUNT(*) FROM replies r WHERE r.post_id = posts.id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
                   (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'like') AS like_count,
                   (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = posts.id OR i.object_url = posts.id) AND i.type = 'boost') AS boost_count
            FROM posts
            WHERE id = ?1
            LIMIT 1
            "#,
        )
        .bind_refs(&id_arg)?
        .first::<Map<String, Value>>(None)
        .await?;
    let Some(post) = post else {
        return Ok(None);
    };
    let replies = owner_post_replies(env, id).await?;
    let likes = owner_post_interactions(env, id, "like").await?;
    let boosts = owner_post_interactions(env, id, "boost").await?;
    Ok(Some(shape_owner_post_detail(post, replies, likes, boosts)))
}

async fn owner_post_replies(env: &Env, id: &str) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare(
        r#"
        SELECT id, actor_id, actor_username, actor_display_name, actor_avatar_url,
               content, published_at, created_at
        FROM replies
        WHERE post_id = ?1 AND (hidden IS NULL OR hidden = 0)
        ORDER BY published_at ASC
        "#,
    )
    .bind_refs(&id_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_post_interactions(
    env: &Env,
    id: &str,
    interaction_type: &str,
) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    let type_arg = D1Type::Text(interaction_type);
    db.prepare(
        r#"
        SELECT id, actor_id, actor_username, actor_display_name, actor_avatar_url,
               object_url, created_at
        FROM interactions
        WHERE (post_id = ?1 OR object_url = ?1) AND type = ?2
        ORDER BY created_at DESC
        "#,
    )
    .bind_refs([&id_arg, &type_arg])?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

fn shape_owner_post_detail(
    post: Map<String, Value>,
    replies: Vec<Map<String, Value>>,
    likes: Vec<Map<String, Value>>,
    boosts: Vec<Map<String, Value>>,
) -> Map<String, Value> {
    let mut detail = Map::new();
    detail.insert("id".to_string(), row_value_or_null(&post, "id"));
    detail.insert("actor_id".to_string(), row_value_or_null(&post, "actor_id"));
    detail.insert("title".to_string(), row_value_or_null(&post, "name"));
    detail.insert(
        "content".to_string(),
        string_value_or_default(&post, "content"),
    );
    detail.insert(
        "content_html".to_string(),
        row_value_or_null(&post, "content_html"),
    );
    detail.insert(
        "visibility".to_string(),
        Value::String(title_visibility(
            string_field(Some(&post), "visibility").as_deref(),
        )),
    );
    detail.insert(
        "protocol".to_string(),
        Value::String(title_protocol(
            string_field(Some(&post), "protocol").as_deref(),
        )),
    );
    detail.insert(
        "encrypted".to_string(),
        Value::Bool(non_empty_value(&post, "encrypted_message").is_some()),
    );
    detail.insert(
        "attachments".to_string(),
        Value::Array(parse_attachment_array(post.get("media_attachments"))),
    );
    detail.insert(
        "in_reply_to".to_string(),
        row_value_or_null(&post, "in_reply_to"),
    );
    detail.insert(
        "published_at".to_string(),
        row_value_or_null(&post, "published_at"),
    );
    detail.insert(
        "reply_count".to_string(),
        Value::from(integer_field(Some(&post), "reply_count")),
    );
    detail.insert(
        "like_count".to_string(),
        Value::from(integer_field(Some(&post), "like_count")),
    );
    detail.insert(
        "boost_count".to_string(),
        Value::from(integer_field(Some(&post), "boost_count")),
    );
    detail.insert(
        "replies".to_string(),
        Value::Array(replies.into_iter().map(Value::Object).collect()),
    );
    detail.insert(
        "likes".to_string(),
        Value::Array(likes.into_iter().map(Value::Object).collect()),
    );
    detail.insert(
        "boosts".to_string(),
        Value::Array(boosts.into_iter().map(Value::Object).collect()),
    );
    detail
}

fn title_visibility(value: Option<&str>) -> String {
    match value {
        Some("public") => "Public",
        Some("unlisted") => "Unlisted",
        Some("direct") => "Direct",
        _ => "Followers",
    }
    .to_string()
}

fn title_protocol(value: Option<&str>) -> String {
    match value {
        Some("atproto") => "AtProto",
        Some("both") => "Both",
        _ => "ActivityPub",
    }
    .to_string()
}

fn parse_attachment_array(value: Option<&Value>) -> Vec<Value> {
    let parsed = match value {
        Some(Value::Array(items)) => Some(items.clone()),
        Some(Value::String(text)) if !text.trim().is_empty() => serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|value| match value {
                Value::Array(items) => Some(items),
                _ => None,
            }),
        _ => None,
    };
    parsed
        .unwrap_or_default()
        .into_iter()
        .filter(|item| {
            item.as_object()
                .and_then(|object| object.get("url"))
                .and_then(Value::as_str)
                .is_some()
        })
        .collect()
}

async fn owner_home_timeline(
    env: &Env,
    limit: i32,
    include_replies: bool,
) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let include_replies_arg = D1Type::Integer(if include_replies { 1 } else { 0 });
    db.prepare(
        r#"
        SELECT id, object_id, actor_id, actor_username, actor_display_name, actor_avatar_url,
               content, content_html, visibility, in_reply_to, published_at, updated_at,
               protocol, created_at,
               (SELECT COUNT(*) FROM replies r WHERE r.post_id = timeline_posts.object_id AND (r.hidden IS NULL OR r.hidden = 0)) AS reply_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = timeline_posts.object_id OR i.object_url = timeline_posts.object_id) AND i.type = 'like') AS like_count,
               (SELECT COUNT(*) FROM interactions i WHERE (i.post_id = timeline_posts.object_id OR i.object_url = timeline_posts.object_id) AND i.type = 'boost') AS boost_count
        FROM timeline_posts
        WHERE deleted_at IS NULL
          AND (?2 = 1 OR in_reply_to IS NULL OR in_reply_to = '')
        ORDER BY published_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs([&limit_arg, &include_replies_arg])?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_notifications(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, type, actor_id, actor_username, actor_display_name, actor_avatar_url,
               post_id, activity_id, content, read, created_at
        FROM notifications
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_mark_notification_read(env: &Env, id: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare("UPDATE notifications SET read = 1 WHERE id = ?1")
        .bind_refs(&id_arg)?
        .run()
        .await?;
    Ok(())
}

async fn owner_deliveries(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, post_id, target_type, target_url, protocol, status, retry_count,
               last_attempt_at, error_message, activity_type, created_at, delivered_at
        FROM deliveries
        ORDER BY created_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_direct_messages(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, conversation_id, sender_id, content, published_at, created_at
        FROM direct_messages
        ORDER BY published_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_search(env: &Env, query: String, limit: i32) -> Result<OwnerSearch> {
    let term = query.trim().to_string();
    if term.is_empty() {
        return Ok(OwnerSearch {
            posts: Vec::new(),
            users: Vec::new(),
            sources: Vec::new(),
            source_items: Vec::new(),
        });
    }

    let db = env.d1("DB")?;
    let like = format!("%{term}%");
    let like_arg = D1Type::Text(&like);
    let limit_arg = D1Type::Integer(limit);
    let posts = db
        .prepare(
            r#"
            SELECT id, actor_id, content, content_html, COALESCE(object_type, 'Note') AS object_type,
                   name, summary, start_time, end_time, location, poll_options,
                   visibility, COALESCE(protocol, 'activitypub') AS protocol,
                   published_at, in_reply_to, atproto_uri, encrypted_message, media_attachments
            FROM posts
            WHERE content LIKE ?1 OR name LIKE ?1 OR summary LIKE ?1
            ORDER BY published_at DESC
            LIMIT ?2
            "#,
        )
        .bind_refs([&like_arg, &limit_arg])?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    let users = db
        .prepare(
            r#"
            SELECT follower_actor_id AS actor_id, 'follower' AS relation, status, created_at
            FROM followers
            WHERE follower_actor_id LIKE ?1
            UNION ALL
            SELECT target_actor_id AS actor_id, 'following' AS relation, status, created_at
            FROM following
            WHERE target_actor_id LIKE ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )
        .bind_refs([&like_arg, &limit_arg])?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    let sources = db
        .prepare(
            r#"
            SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
                   last_fetched_at, next_fetch_at, last_error, error_count, policy_json,
                   created_at, updated_at
            FROM source_subscriptions
            WHERE url LIKE ?1 OR title LIKE ?1 OR homepage_url LIKE ?1
            ORDER BY updated_at DESC
            LIMIT ?2
            "#,
        )
        .bind_refs([&like_arg, &limit_arg])?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    let source_items = db
        .prepare(
            r#"
            SELECT id, source_id, source_type, title, canonical_url, excerpt, published_at,
                   read, rights_policy_json, created_at
            FROM source_items
            WHERE title LIKE ?1 OR canonical_url LIKE ?1 OR excerpt LIKE ?1
            ORDER BY COALESCE(published_at, created_at) DESC
            LIMIT ?2
            "#,
        )
        .bind_refs([&like_arg, &limit_arg])?
        .all()
        .await?
        .results::<Map<String, Value>>()?;

    Ok(OwnerSearch {
        posts,
        users,
        sources,
        source_items,
    })
}

async fn owner_source_subscriptions(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    db.prepare(
        r#"
        SELECT id, source_type, url, title, homepage_url, status, refresh_cadence_minutes,
               last_fetched_at, next_fetch_at, last_error, error_count, policy_json, created_at, updated_at
        FROM source_subscriptions
        ORDER BY updated_at DESC
        LIMIT ?1
        "#,
    )
    .bind_refs(&limit_arg)?
    .all()
    .await?
    .results::<Map<String, Value>>()
}

async fn owner_source_items(env: &Env, limit: i32) -> Result<Vec<Map<String, Value>>> {
    let db = env.d1("DB")?;
    let limit_arg = D1Type::Integer(limit);
    let rows = db
        .prepare(
            r#"
            SELECT id, source_id, source_type, title, canonical_url, external_id, author,
                   published_at, fetched_at, excerpt, content_type, thumbnail_url,
                   rights_policy_json, read, summary, created_at, updated_at
            FROM source_items
            ORDER BY COALESCE(published_at, fetched_at) DESC
            LIMIT ?1
            "#,
        )
        .bind_refs(&limit_arg)?
        .all()
        .await?
        .results::<Map<String, Value>>()?;
    Ok(rows.into_iter().map(normalize_source_item).collect())
}

async fn owner_delete_source(env: &Env, id: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let id_arg = D1Type::Text(id);
    db.prepare("DELETE FROM source_subscriptions WHERE id = ?1")
        .bind_refs(&id_arg)?
        .run()
        .await?;
    Ok(())
}

fn normalize_source_item(row: Map<String, Value>) -> Map<String, Value> {
    let mut item = Map::new();
    item.insert("id".to_string(), row_value_or_null(&row, "id"));
    item.insert("title".to_string(), row_value_or_null(&row, "title"));
    item.insert(
        "source_type".to_string(),
        row_value_or_null(&row, "source_type"),
    );
    item.insert(
        "canonical_url".to_string(),
        row_value_or_null(&row, "canonical_url"),
    );
    item.insert(
        "excerpt".to_string(),
        row_value_or_fallback_null(&row, "excerpt", "summary"),
    );
    item.insert(
        "rights_policy_json".to_string(),
        non_empty_value(&row, "rights_policy_json")
            .unwrap_or_else(|| Value::String("{}".to_string())),
    );
    item.insert(
        "read".to_string(),
        Value::Bool(bool_field(Some(&row), "read")),
    );
    item.insert(
        "source_id".to_string(),
        row_value_or_null(&row, "source_id"),
    );
    item.insert("author".to_string(), row_value_or_null(&row, "author"));
    item.insert(
        "published_at".to_string(),
        row_value_or_null(&row, "published_at"),
    );
    item.insert(
        "fetched_at".to_string(),
        row_value_or_null(&row, "fetched_at"),
    );
    item.insert(
        "thumbnail_url".to_string(),
        row_value_or_null(&row, "thumbnail_url"),
    );
    item
}

async fn owner_local_actor(env: &Env) -> Result<LocalActor> {
    let db = env.d1("DB")?;
    let row = db
        .prepare("SELECT id, username FROM actors WHERE username = 'social' LIMIT 1")
        .first::<Map<String, Value>>(None)
        .await?;
    Ok(LocalActor {
        id: string_field(row.as_ref(), "id")
            .unwrap_or_else(|| "https://social.dais.social/users/social".to_string()),
    })
}

fn query_param(url: &worker::Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
}

fn decode_component(value: &str) -> String {
    urlencoding::decode(value)
        .map(|decoded| decoded.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

async fn read_json(req: &mut Request) -> Value {
    req.json::<Value>()
        .await
        .unwrap_or_else(|_| serde_json::json!({}))
}

fn required_body_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(Value::Number(number)) if number.as_i64().unwrap_or(1) != 0 => {
            Some(number.to_string())
        }
        Some(Value::Bool(true)) => Some("true".to_string()),
        _ => None,
    }
}

fn clamp_limit(value: Option<String>) -> i32 {
    value
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(20)
        .clamp(1, 80)
}

fn string_field(row: Option<&Map<String, Value>>, key: &str) -> Option<String> {
    row.and_then(|fields| fields.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn integer_field(row: Option<&Map<String, Value>>, key: &str) -> i64 {
    row.and_then(|fields| fields.get(key))
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()))
                .or_else(|| value.as_f64().map(|number| number as i64))
                .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()))
        })
        .unwrap_or(0)
}

fn bool_field(row: Option<&Map<String, Value>>, key: &str) -> bool {
    row.and_then(|fields| fields.get(key))
        .and_then(|value| {
            value
                .as_bool()
                .or_else(|| value.as_i64().map(|number| number != 0))
                .or_else(|| value.as_u64().map(|number| number != 0))
                .or_else(|| {
                    value
                        .as_str()
                        .map(|text| matches!(text, "1" | "true" | "TRUE" | "yes" | "YES"))
                })
        })
        .unwrap_or(false)
}

fn row_value_or_null(row: &Map<String, Value>, key: &str) -> Value {
    non_empty_value(row, key).unwrap_or(Value::Null)
}

fn string_value_or_default(row: &Map<String, Value>, key: &str) -> Value {
    string_field(Some(row), key)
        .map(Value::String)
        .unwrap_or_else(|| Value::String(String::new()))
}

fn row_value_or_fallback_null(row: &Map<String, Value>, key: &str, fallback: &str) -> Value {
    non_empty_value(row, key)
        .or_else(|| non_empty_value(row, fallback))
        .unwrap_or(Value::Null)
}

fn non_empty_value(row: &Map<String, Value>, key: &str) -> Option<Value> {
    row.get(key).and_then(|value| match value {
        Value::Null => None,
        Value::String(text) if text.is_empty() => None,
        _ => Some(value.clone()),
    })
}

fn require_owner_bearer(
    req: &Request,
    env: &Env,
    required_scopes: &[&str],
) -> Result<Option<Response>> {
    let tokens = owner_bearer_tokens(env);
    if tokens.is_empty()
        && env
            .var("ENVIRONMENT")
            .map(|value| value.to_string() == "production")
            .unwrap_or(false)
    {
        return Ok(Some(api_json(
            &serde_json::json!({ "error": "OWNER_API_TOKEN is not configured" }),
            503,
        )?));
    }
    let auth = req.headers().get("Authorization")?.unwrap_or_default();
    let provided = auth.strip_prefix("Bearer ").map(str::trim).unwrap_or("");
    let token = tokens.iter().find(|entry| entry.token == provided);
    match token {
        Some(entry) if owner_token_has_scopes(&entry.scopes, required_scopes) => Ok(None),
        Some(_) => Ok(Some(api_json(
            &serde_json::json!({
                "error": "Owner bearer token lacks required scope",
                "required_scopes": required_scopes,
            }),
            403,
        )?)),
        None => Ok(Some(api_json(
            &serde_json::json!({ "error": "Owner bearer token required" }),
            401,
        )?)),
    }
}

fn owner_bearer_tokens(env: &Env) -> Vec<OwnerToken> {
    let mut tokens = Vec::new();
    let configured = env
        .var("OWNER_API_TOKEN")
        .or_else(|_| env.var("DAIS_OWNER_TOKEN"))
        .map(|value| value.to_string())
        .unwrap_or_else(|_| {
            if env
                .var("ENVIRONMENT")
                .map(|value| value.to_string() == "production")
                .unwrap_or(false)
            {
                String::new()
            } else {
                "dais-local-owner-token".to_string()
            }
        });
    if !configured.is_empty() {
        tokens.push(OwnerToken {
            token: configured,
            scopes: vec!["owner".to_string()],
        });
    }
    tokens.extend(scoped_owner_tokens(env));
    tokens
}

fn scoped_owner_tokens(env: &Env) -> Vec<OwnerToken> {
    let raw = env
        .var("OWNER_API_SCOPED_TOKENS")
        .or_else(|_| env.var("DAIS_OWNER_SCOPED_TOKENS"))
        .map(|value| value.to_string())
        .unwrap_or_default();
    if raw.trim().is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(map)) => map
            .into_iter()
            .filter_map(|(token, scopes)| {
                let scopes = normalize_scopes(scopes);
                if token.trim().is_empty() || scopes.is_empty() {
                    None
                } else {
                    Some(OwnerToken { token, scopes })
                }
            })
            .collect(),
        Ok(Value::Array(values)) => values
            .into_iter()
            .filter_map(|value| {
                let token = value
                    .get("token")
                    .or_else(|| value.get("value"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let scopes = normalize_scopes(
                    value
                        .get("scopes")
                        .or_else(|| value.get("scope"))
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                if token.is_empty() || scopes.is_empty() {
                    None
                } else {
                    Some(OwnerToken { token, scopes })
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_scopes(value: Value) -> Vec<String> {
    match value {
        Value::Array(values) => values
            .into_iter()
            .filter_map(|value| value.as_str().map(normalize_scope))
            .filter(|scope| !scope.is_empty())
            .collect(),
        Value::String(scopes) => scopes
            .split(|character: char| character == ',' || character.is_whitespace())
            .map(normalize_scope)
            .filter(|scope| !scope.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_scope(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn owner_token_has_scopes(scopes: &[String], required_scopes: &[&str]) -> bool {
    scopes
        .iter()
        .any(|scope| scope == "owner" || scope == "admin" || scope == "*")
        || required_scopes
            .iter()
            .all(|required| scopes.iter().any(|scope| scope == required))
}

fn api_json<T: Serialize>(value: &T, status: u16) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Content-Type", "application/json; charset=utf-8")?;
    headers.set("Access-Control-Allow-Origin", "*")?;
    headers.set(
        "Access-Control-Allow-Headers",
        "Authorization, Content-Type",
    )?;
    headers.set(
        "Access-Control-Allow-Methods",
        "GET, POST, PUT, PATCH, DELETE, OPTIONS",
    )?;
    let mut response = if status == 204 {
        Response::empty()?.with_status(status)
    } else {
        Response::from_json(value)?.with_status(status)
    };
    response = response.with_headers(headers);
    Ok(response)
}

fn fixture_actor_response(url: &worker::Url) -> Result<Response> {
    let public_key = match fixture_public_key(url) {
        Some(value) => value,
        None => return Response::error("Missing or invalid fixture public key", 400),
    };
    let actor_url = url.to_string();
    let name = url
        .query_pairs()
        .find(|(key, _)| key == "name")
        .map(|(_, value)| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "dais-s2s-fixture".to_string());
    activity_json(&FixtureActor {
        context: "https://www.w3.org/ns/activitystreams",
        id: &actor_url,
        actor_type: "Application",
        preferred_username: &name,
        name: &name,
        inbox: &format!(
            "{}://{}/__dais-fixtures/activitypub/inbox",
            url.scheme(),
            url.host_str().unwrap_or_default()
        ),
        outbox: &fixture_url_with_public_key(url, "/__dais-fixtures/activitypub/outbox"),
        public_key: FixturePublicKey {
            id: &format!("{actor_url}#main-key"),
            owner: &actor_url,
            public_key_pem: &public_key,
        },
    })
}

fn fixture_outbox_response(url: &worker::Url) -> Result<Response> {
    let post = fixture_post(url);
    let create_id = format!("{}#create", post.id);
    activity_json(&FixtureOutbox {
        context: "https://www.w3.org/ns/activitystreams",
        id: &url.to_string(),
        collection_type: "OrderedCollection",
        total_items: 1,
        ordered_items: vec![FixtureCreate {
            id: &create_id,
            create_type: "Create",
            actor: post.attributed_to.clone(),
            to: post.to.clone(),
            object: post,
        }],
    })
}

fn fixture_post_response(url: &worker::Url) -> Result<Response> {
    activity_json(&fixture_post(url))
}

fn fixture_post(url: &worker::Url) -> FixturePost {
    let post_id =
        fixture_url_with_public_key(url, "/__dais-fixtures/activitypub/posts/public-preview");
    FixturePost {
        context: "https://www.w3.org/ns/activitystreams",
        id: post_id.clone(),
        post_type: "Note",
        attributed_to: fixture_url_with_public_key(url, "/__dais-fixtures/activitypub/actor"),
        to: vec![PUBLIC_COLLECTION.to_string()],
        content: "<p>Dais fixture public preview post</p>",
        published: "2026-06-16T00:00:00Z",
        url: post_id,
    }
}

fn activity_json<T: Serialize>(value: &T) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
    headers.set("Cache-Control", "no-store")?;
    Ok(Response::from_json(value)?.with_headers(headers))
}

fn fixture_public_key(url: &worker::Url) -> Option<String> {
    let encoded = url
        .query_pairs()
        .find(|(key, _)| key == "pk")
        .map(|(_, value)| value.to_string())?;
    if encoded.len() > 2000
        || !encoded
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return None;
    }
    let base64 = encoded.replace('-', "+").replace('_', "/");
    let decoded = base64_decode(&base64)?;
    let pem = String::from_utf8(decoded).ok()?;
    if pem.contains("-----BEGIN PUBLIC KEY-----") && pem.contains("-----END PUBLIC KEY-----") {
        Some(pem)
    } else {
        None
    }
}

fn base64_decode(value: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;

    for byte in value.bytes().filter(|byte| *byte != b'=') {
        let index = TABLE.iter().position(|candidate| *candidate == byte)? as u32;
        buffer = (buffer << 6) | index;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }

    Some(output)
}

fn fixture_url_with_public_key(url: &worker::Url, path: &str) -> String {
    let mut next = url.join(path).unwrap_or_else(|_| {
        worker::Url::parse(&format!(
            "{}://{}{}",
            url.scheme(),
            url.host_str().unwrap_or_default(),
            path
        ))
        .expect("fixture URL")
    });
    if let Some(public_key) = url
        .query_pairs()
        .find(|(key, _)| key == "pk")
        .map(|(_, value)| value.to_string())
    {
        next.query_pairs_mut().append_pair("pk", &public_key);
    }
    next.to_string()
}

#[derive(Serialize)]
struct FixtureActor<'a> {
    #[serde(rename = "@context")]
    context: &'a str,
    id: &'a str,
    #[serde(rename = "type")]
    actor_type: &'a str,
    #[serde(rename = "preferredUsername")]
    preferred_username: &'a str,
    name: &'a str,
    inbox: &'a str,
    outbox: &'a str,
    #[serde(rename = "publicKey")]
    public_key: FixturePublicKey<'a>,
}

#[derive(Serialize)]
struct FixturePublicKey<'a> {
    id: &'a str,
    owner: &'a str,
    #[serde(rename = "publicKeyPem")]
    public_key_pem: &'a str,
}

#[derive(Clone, Serialize)]
struct FixturePost {
    #[serde(rename = "@context")]
    context: &'static str,
    id: String,
    #[serde(rename = "type")]
    post_type: &'static str,
    #[serde(rename = "attributedTo")]
    attributed_to: String,
    to: Vec<String>,
    content: &'static str,
    published: &'static str,
    url: String,
}

#[derive(Serialize)]
struct FixtureCreate<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    create_type: &'a str,
    actor: String,
    to: Vec<String>,
    object: FixturePost,
}

#[derive(Serialize)]
struct FixtureOutbox<'a> {
    #[serde(rename = "@context")]
    context: &'a str,
    id: &'a str,
    #[serde(rename = "type")]
    collection_type: &'a str,
    #[serde(rename = "totalItems")]
    total_items: u8,
    #[serde(rename = "orderedItems")]
    ordered_items: Vec<FixtureCreate<'a>>,
}

#[derive(Serialize)]
struct OwnerProfile {
    id: String,
    username: String,
    actor_type: String,
    display_name: Option<String>,
    summary: Option<String>,
    icon: Option<String>,
    image: Option<String>,
    avatar_url: Option<String>,
    header_url: Option<String>,
    public_handle: String,
    actor_url: String,
}

#[derive(Serialize)]
struct OwnerStats {
    followers_total: i64,
    followers_approved: i64,
    followers_pending: i64,
    followers_rejected: i64,
    following_total: i64,
    posts_total: i64,
    activities_total: i64,
    deliveries_total: i64,
    deliveries_failed: i64,
    deliveries_queued: i64,
    deliveries_retry: i64,
    deliveries_delivered: i64,
    dual_protocol_posts: i64,
    public_posts: i64,
    private_posts: i64,
    direct_posts: i64,
    encrypted_posts: i64,
    media_posts: i64,
    notifications_unread: i64,
    blocks_total: i64,
    allowlist_hosts: i64,
    closed_network: bool,
}

#[derive(Serialize)]
struct OwnerItems<T> {
    items: Vec<T>,
}

#[derive(Serialize)]
struct OwnerSearch {
    posts: Vec<Map<String, Value>>,
    users: Vec<Map<String, Value>>,
    sources: Vec<Map<String, Value>>,
    source_items: Vec<Map<String, Value>>,
}

#[derive(Serialize)]
struct OwnerSources {
    subscriptions: Vec<Map<String, Value>>,
    items: Vec<Map<String, Value>>,
}

#[derive(Serialize)]
struct OwnerModeration {
    closed_network: bool,
    block_count: i64,
    allowlist_count: i64,
    require_authorized_fetch: bool,
    manually_approves_followers: bool,
    blocks: Vec<Map<String, Value>>,
    allowlist: Vec<Map<String, Value>>,
}

#[derive(Serialize)]
struct OwnerDiagnostic {
    key: &'static str,
    ok: bool,
    detail: String,
}

#[derive(Deserialize)]
struct DeliveryCount {
    status: String,
    count: i64,
}

struct LocalActor {
    id: String,
}

struct OwnerToken {
    token: String,
    scopes: Vec<String>,
}
