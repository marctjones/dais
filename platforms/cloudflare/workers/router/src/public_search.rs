use crate::moderation::detect_sensitive_categories;
use crate::request::{optional_body_string, query_param};
use crate::{
    actor_handle, discover_public_post_target, fetch_json_with_accept,
    fetch_lenient_json_with_accept, normalize_host_value, resolve_activitypub_actor, string_field,
    strip_html,
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashSet;
use worker::{D1Type, Env, Result};

#[derive(Serialize)]
pub(crate) struct OwnerSearch {
    posts: Vec<Map<String, Value>>,
    users: Vec<Map<String, Value>>,
    sources: Vec<Map<String, Value>>,
    source_items: Vec<Map<String, Value>>,
    public_posts: Vec<Map<String, Value>>,
    public_actors: Vec<Map<String, Value>>,
    provider_errors: Vec<Map<String, Value>>,
    public_search_guard: OwnerPublicSearchGuard,
}

#[derive(Default, Serialize)]
struct OwnerPublicSearchGuard {
    blocked: bool,
    requires_confirmation: bool,
    confirmed: bool,
    categories: Vec<String>,
    message: Option<String>,
}

#[derive(Clone)]
pub(crate) struct OwnerSearchFlags {
    include_local: bool,
    include_public: bool,
    confirm_public_sensitive: bool,
    public_options: OwnerPublicSearchOptions,
}

pub(crate) fn owner_search_flags(url: &worker::Url) -> OwnerSearchFlags {
    let scope = query_param(url, "scope")
        .unwrap_or_else(|| "local".to_string())
        .to_ascii_lowercase();
    let mut include_local = matches!(scope.as_str(), "" | "local" | "all");
    let mut include_public = matches!(scope.as_str(), "public" | "remote" | "all");

    if query_param(url, "include_public").as_deref() == Some("true") {
        include_public = true;
    }
    if query_param(url, "include_local").as_deref() == Some("false") {
        include_local = false;
    }
    let confirm_public_sensitive = matches!(
        query_param(url, "confirm_public_sensitive").as_deref(),
        Some("true" | "1" | "yes" | "on")
    );

    OwnerSearchFlags {
        include_local,
        include_public,
        confirm_public_sensitive,
        public_options: OwnerPublicSearchOptions::from_url(url),
    }
}

pub(crate) async fn owner_search(
    env: &Env,
    query: String,
    limit: i32,
    flags: OwnerSearchFlags,
) -> Result<OwnerSearch> {
    let term = query.trim().to_string();
    if term.is_empty() {
        return Ok(OwnerSearch {
            posts: Vec::new(),
            users: Vec::new(),
            sources: Vec::new(),
            source_items: Vec::new(),
            public_posts: Vec::new(),
            public_actors: Vec::new(),
            provider_errors: Vec::new(),
            public_search_guard: OwnerPublicSearchGuard::default(),
        });
    }

    let (posts, users, sources, source_items) = if flags.include_local {
        owner_local_search(env, &term, limit).await?
    } else {
        (Vec::new(), Vec::new(), Vec::new(), Vec::new())
    };
    let public_categories = if flags.include_public {
        detect_sensitive_categories(&term)
    } else {
        Vec::new()
    };
    let public_guard =
        owner_public_search_guard(&public_categories, flags.confirm_public_sensitive);
    let public = if flags.include_public && !public_guard.blocked {
        owner_public_search(env, &term, limit, &flags.public_options).await
    } else {
        OwnerPublicSearch::default()
    };

    Ok(OwnerSearch {
        posts,
        users,
        sources,
        source_items,
        public_posts: public.posts,
        public_actors: public.actors,
        provider_errors: public.provider_errors,
        public_search_guard: public_guard,
    })
}

fn owner_public_search_guard(
    categories: &[String],
    confirm_public_sensitive: bool,
) -> OwnerPublicSearchGuard {
    let requires_confirmation = !categories.is_empty();
    let confirmed = requires_confirmation && confirm_public_sensitive;
    let blocked = requires_confirmation && !confirmed;
    let message = if blocked {
        Some("Public provider search skipped until the operator confirms this sensitive query.")
    } else if confirmed {
        Some("Sensitive public search was explicitly confirmed by the operator.")
    } else {
        None
    };
    OwnerPublicSearchGuard {
        blocked,
        requires_confirmation,
        confirmed,
        categories: categories.to_vec(),
        message: message.map(str::to_string),
    }
}

type OwnerLocalSearchRows = (
    Vec<Map<String, Value>>,
    Vec<Map<String, Value>>,
    Vec<Map<String, Value>>,
    Vec<Map<String, Value>>,
);

async fn owner_local_search(env: &Env, term: &str, limit: i32) -> Result<OwnerLocalSearchRows> {
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

    Ok((posts, users, sources, source_items))
}

const BLUESKY_APPVIEW_BASE_URL: &str = "https://api.bsky.app";
const TOOTFINDER_SEARCH_BASE_URL: &str = "https://www.tootfinder.ch/rest/api/search";
pub(crate) const MAX_ACTIVITYPUB_SEARCH_SERVERS: usize = 5;
const DEFAULT_ACTIVITYPUB_SEARCH_SERVERS: &[&str] =
    &["mastodon.social", "mstdn.social", "fosstodon.org"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum OwnerPublicSearchProvider {
    All,
    Bluesky,
    ActivityPub,
    Tootfinder,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum OwnerPublicSearchResultType {
    All,
    Posts,
    Actors,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct OwnerPublicSearchFilters {
    pub(crate) sort: Option<String>,
    pub(crate) since: Option<String>,
    pub(crate) until: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) mentions: Option<String>,
    pub(crate) lang: Option<String>,
    pub(crate) domain: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) tags: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OwnerPublicSearchOptions {
    pub(crate) provider: OwnerPublicSearchProvider,
    pub(crate) result_type: OwnerPublicSearchResultType,
    pub(crate) activitypub_servers: Vec<String>,
    pub(crate) filters: OwnerPublicSearchFilters,
}

impl Default for OwnerPublicSearchOptions {
    fn default() -> Self {
        Self {
            provider: OwnerPublicSearchProvider::All,
            result_type: OwnerPublicSearchResultType::All,
            activitypub_servers: Vec::new(),
            filters: OwnerPublicSearchFilters::default(),
        }
    }
}

impl OwnerPublicSearchOptions {
    pub(crate) fn from_url(url: &worker::Url) -> Self {
        Self {
            provider: owner_public_search_provider(
                query_param(url, "provider").as_deref().unwrap_or("all"),
            ),
            result_type: owner_public_search_result_type(
                query_param(url, "type")
                    .or_else(|| query_param(url, "result_type"))
                    .as_deref()
                    .unwrap_or("all"),
            ),
            activitypub_servers: public_search_query_values(
                url,
                &[
                    "server",
                    "servers",
                    "activitypub_server",
                    "activitypub_servers",
                ],
            )
            .into_iter()
            .filter_map(|value| normalize_host_value(&value).ok())
            .take(MAX_ACTIVITYPUB_SEARCH_SERVERS)
            .collect(),
            filters: OwnerPublicSearchFilters {
                sort: public_search_sort(query_param(url, "sort")),
                since: non_empty_query_param(url, "since"),
                until: non_empty_query_param(url, "until"),
                author: non_empty_query_param(url, "author"),
                mentions: non_empty_query_param(url, "mentions"),
                lang: non_empty_query_param(url, "lang"),
                domain: non_empty_query_param(url, "domain"),
                url: non_empty_query_param(url, "url"),
                tags: public_search_query_values(url, &["tag", "tags"])
                    .into_iter()
                    .map(|value| value.trim().trim_start_matches('#').to_string())
                    .filter(|value| !value.is_empty())
                    .take(8)
                    .collect(),
            },
        }
    }

    pub(crate) fn includes_bluesky(&self) -> bool {
        matches!(
            self.provider,
            OwnerPublicSearchProvider::All | OwnerPublicSearchProvider::Bluesky
        )
    }

    pub(crate) fn includes_activitypub(&self) -> bool {
        matches!(
            self.provider,
            OwnerPublicSearchProvider::All | OwnerPublicSearchProvider::ActivityPub
        )
    }

    pub(crate) fn includes_tootfinder(&self) -> bool {
        matches!(
            self.provider,
            OwnerPublicSearchProvider::All
                | OwnerPublicSearchProvider::ActivityPub
                | OwnerPublicSearchProvider::Tootfinder
        )
    }

    pub(crate) fn includes_posts(&self) -> bool {
        matches!(
            self.result_type,
            OwnerPublicSearchResultType::All | OwnerPublicSearchResultType::Posts
        )
    }

    pub(crate) fn includes_actors(&self) -> bool {
        matches!(
            self.result_type,
            OwnerPublicSearchResultType::All | OwnerPublicSearchResultType::Actors
        )
    }
}

fn owner_public_search_provider(value: &str) -> OwnerPublicSearchProvider {
    match value.trim().to_ascii_lowercase().as_str() {
        "bluesky" | "bsky" | "atproto" | "at" => OwnerPublicSearchProvider::Bluesky,
        "activitypub" | "ap" | "mastodon" | "fediverse" => OwnerPublicSearchProvider::ActivityPub,
        "tootfinder" | "tootfinder.ch" | "activitypub-index" | "activitypub_index" | "index" => {
            OwnerPublicSearchProvider::Tootfinder
        }
        _ => OwnerPublicSearchProvider::All,
    }
}

fn owner_public_search_result_type(value: &str) -> OwnerPublicSearchResultType {
    match value.trim().to_ascii_lowercase().as_str() {
        "post" | "posts" | "status" | "statuses" => OwnerPublicSearchResultType::Posts,
        "actor" | "actors" | "account" | "accounts" | "profile" | "profiles" => {
            OwnerPublicSearchResultType::Actors
        }
        _ => OwnerPublicSearchResultType::All,
    }
}

fn public_search_sort(value: Option<String>) -> Option<String> {
    match value
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("top") => Some("top".to_string()),
        Some("latest") | Some("recent") | Some("new") => Some("latest".to_string()),
        _ => None,
    }
}

fn non_empty_query_param(url: &worker::Url, key: &str) -> Option<String> {
    query_param(url, key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn public_search_query_values(url: &worker::Url, keys: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    for (name, value) in url.query_pairs() {
        if !keys.iter().any(|key| name == *key) {
            continue;
        }
        for part in value.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                values.push(trimmed.to_string());
            }
        }
    }
    values
}

pub(crate) fn owner_public_search_mastodon_query_params(
    term: &str,
    limit: i32,
    result_type: &OwnerPublicSearchResultType,
) -> Vec<(String, String)> {
    let mut params = vec![
        ("q".to_string(), term.to_string()),
        ("limit".to_string(), limit.to_string()),
    ];
    match result_type {
        OwnerPublicSearchResultType::Posts => {
            params.push(("type".to_string(), "statuses".to_string()));
        }
        OwnerPublicSearchResultType::Actors => {
            params.push(("type".to_string(), "accounts".to_string()));
        }
        OwnerPublicSearchResultType::All => {}
    }
    params
}

pub(crate) fn bluesky_appview_xrpc_url(method: &str, query: &str) -> String {
    let base = format!("{BLUESKY_APPVIEW_BASE_URL}/xrpc/{method}");
    if query.is_empty() {
        base
    } else {
        format!("{base}?{query}")
    }
}

pub(crate) fn tootfinder_search_url(term: &str) -> String {
    format!(
        "{TOOTFINDER_SEARCH_BASE_URL}/{}",
        urlencoding::encode(term.trim())
    )
}

pub(crate) fn tootfinder_search_items(body: &Value) -> Vec<Value> {
    if let Some(items) = body.as_array() {
        return items.clone();
    }
    body.get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

#[derive(Default)]
pub(crate) struct OwnerPublicSearch {
    pub(crate) posts: Vec<Map<String, Value>>,
    pub(crate) actors: Vec<Map<String, Value>>,
    pub(crate) provider_errors: Vec<Map<String, Value>>,
}

pub(crate) async fn owner_public_search(
    env: &Env,
    term: &str,
    limit: i32,
    options: &OwnerPublicSearchOptions,
) -> OwnerPublicSearch {
    let limit = limit.clamp(1, 25);
    let mut results = OwnerPublicSearch::default();

    if options.includes_bluesky() && options.includes_posts() {
        match owner_public_search_bluesky_posts(term, limit, &options.filters).await {
            Ok(posts) => results.posts.extend(posts),
            Err(error) => results
                .provider_errors
                .push(owner_search_provider_error("bluesky", "atproto", &error)),
        }
    }

    if options.includes_bluesky() && options.includes_actors() {
        match owner_public_search_bluesky_actors(term, limit).await {
            Ok(actors) => results.actors.extend(actors),
            Err(error) => results
                .provider_errors
                .push(owner_search_provider_error("bluesky", "atproto", &error)),
        }
    }

    if options.includes_activitypub() {
        if options.includes_posts() {
            if let Some(post) = owner_public_search_activitypub_direct_post(term).await {
                results.posts.push(post);
            }
        }
        if options.includes_actors() {
            if let Some(actor) = owner_public_search_activitypub_direct_actor(term).await {
                results.actors.push(actor);
            }
        }
        for server in activitypub_search_servers(env, options) {
            match owner_public_search_mastodon(&server, term, limit, options).await {
                Ok((posts, actors)) => {
                    results.posts.extend(posts);
                    results.actors.extend(actors);
                }
                Err(error) => results.provider_errors.push(owner_search_provider_error(
                    &server,
                    "activitypub",
                    &error,
                )),
            }
        }
    }
    if options.includes_tootfinder() && options.includes_posts() {
        match owner_public_search_tootfinder(term, limit).await {
            Ok(posts) => results.posts.extend(posts),
            Err(error) => results.provider_errors.push(owner_search_provider_error(
                "tootfinder.ch",
                "activitypub",
                &error,
            )),
        }
    }

    dedupe_owner_public_search(&mut results);
    results
}

async fn owner_public_search_bluesky_posts(
    term: &str,
    limit: i32,
    filters: &OwnerPublicSearchFilters,
) -> std::result::Result<Vec<Map<String, Value>>, String> {
    let mut params = vec![
        ("q".to_string(), term.to_string()),
        ("limit".to_string(), limit.to_string()),
    ];
    if let Some(sort) = filters.sort.as_deref() {
        params.push(("sort".to_string(), sort.to_string()));
    }
    for (key, value) in [
        ("since", filters.since.as_deref()),
        ("until", filters.until.as_deref()),
        ("author", filters.author.as_deref()),
        ("mentions", filters.mentions.as_deref()),
        ("lang", filters.lang.as_deref()),
        ("domain", filters.domain.as_deref()),
        ("url", filters.url.as_deref()),
    ] {
        if let Some(value) = value {
            params.push((key.to_string(), value.to_string()));
        }
    }
    for tag in &filters.tags {
        params.push(("tag".to_string(), tag.to_string()));
    }
    let url = bluesky_appview_xrpc_url("app.bsky.feed.searchPosts", &encoded_query(&params));
    let body = fetch_json_with_accept(&url, "application/json", "bluesky post search").await?;
    Ok(body
        .get("posts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(owner_normalize_bluesky_post)
        .collect())
}

async fn owner_public_search_bluesky_actors(
    term: &str,
    limit: i32,
) -> std::result::Result<Vec<Map<String, Value>>, String> {
    let url = format!(
        "{}?q={}&limit={}",
        bluesky_appview_xrpc_url("app.bsky.actor.searchActors", ""),
        urlencoding::encode(term),
        limit
    );
    let body = fetch_json_with_accept(&url, "application/json", "bluesky actor search").await?;
    Ok(body
        .get("actors")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(owner_normalize_bluesky_actor)
        .collect())
}

async fn owner_public_search_mastodon(
    server: &str,
    term: &str,
    limit: i32,
    options: &OwnerPublicSearchOptions,
) -> std::result::Result<(Vec<Map<String, Value>>, Vec<Map<String, Value>>), String> {
    let params = owner_public_search_mastodon_query_params(term, limit, &options.result_type);
    let url = format!("https://{server}/api/v2/search?{}", encoded_query(&params));
    let body =
        fetch_json_with_accept(&url, "application/json", "mastodon-compatible search").await?;
    let posts = body
        .get("statuses")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|value| owner_normalize_mastodon_status(server, value))
        .collect();
    let actors = body
        .get("accounts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|value| owner_normalize_mastodon_account(server, value))
        .collect();
    Ok((posts, actors))
}

fn activitypub_search_servers(env: &Env, options: &OwnerPublicSearchOptions) -> Vec<String> {
    let mut servers = Vec::new();
    if !options.activitypub_servers.is_empty() {
        servers.extend(options.activitypub_servers.iter().cloned());
    } else if let Ok(configured) = env.var("DAIS_ACTIVITYPUB_SEARCH_SERVERS") {
        for value in configured.to_string().split(',') {
            if let Ok(host) = normalize_host_value(value) {
                servers.push(host.to_string());
            }
        }
    }
    if servers.is_empty() {
        servers.extend(
            DEFAULT_ACTIVITYPUB_SEARCH_SERVERS
                .iter()
                .map(|value| (*value).to_string()),
        );
    }
    let mut seen = HashSet::new();
    servers
        .into_iter()
        .filter(|server| seen.insert(server.clone()))
        .take(MAX_ACTIVITYPUB_SEARCH_SERVERS)
        .collect()
}

async fn owner_public_search_tootfinder(
    term: &str,
    limit: i32,
) -> std::result::Result<Vec<Map<String, Value>>, String> {
    let url = tootfinder_search_url(term);
    let body =
        fetch_lenient_json_with_accept(&url, "application/json", "tootfinder search").await?;
    Ok(tootfinder_search_items(&body)
        .into_iter()
        .filter_map(owner_normalize_tootfinder_status)
        .take(limit.max(0) as usize)
        .collect())
}

fn encoded_query(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

async fn owner_public_search_activitypub_direct_post(term: &str) -> Option<Map<String, Value>> {
    let discovered = discover_public_post_target(term).await?;
    Some(owner_public_post_row_from_discovered("direct", &discovered))
}

async fn owner_public_search_activitypub_direct_actor(term: &str) -> Option<Map<String, Value>> {
    let remote = resolve_activitypub_actor(term).await.ok()?;
    let handle = actor_handle(&remote);
    let summary = remote.summary.as_deref().map(strip_html);
    let mut row = owner_public_actor_row(
        "direct",
        "activitypub",
        &remote.id,
        handle.clone(),
        remote.name.clone(),
        summary,
        remote.url.clone(),
        remote.icon_url.clone(),
    );
    insert_optional_string(
        &mut row,
        "watch_type",
        Some("activitypub_actor".to_string()),
    );
    insert_optional_string(
        &mut row,
        "watch_target",
        remote.url.clone().or_else(|| Some(remote.id.clone())),
    );
    insert_optional_string(
        &mut row,
        "follow_target",
        remote.url.clone().or_else(|| Some(remote.id.clone())),
    );
    row.insert(
        "actions".to_string(),
        Value::Array(vec![
            Value::String("watch".to_string()),
            Value::String("follow".to_string()),
            Value::String("open".to_string()),
        ]),
    );
    Some(row)
}

pub(crate) fn owner_public_post_row_from_discovered(
    provider: &str,
    discovered: &Map<String, Value>,
) -> Map<String, Value> {
    let id = string_field(Some(discovered), "id").unwrap_or_default();
    let url = string_field(Some(discovered), "url").unwrap_or_else(|| id.clone());
    let mut row = owner_public_post_row(OwnerPublicPostFields {
        provider: provider.to_string(),
        network: "activitypub".to_string(),
        id: id.clone(),
        url: url.clone(),
        actor_id: string_field(Some(discovered), "actor_id"),
        actor_handle: None,
        actor_display_name: None,
        content: string_field(Some(discovered), "content")
            .or_else(|| string_field(Some(discovered), "summary"))
            .unwrap_or_default(),
        content_html: None,
        summary: string_field(Some(discovered), "summary"),
        object_type: string_field(Some(discovered), "type"),
        published_at: string_field(Some(discovered), "published"),
    });
    owner_add_public_post_actions(
        &mut row,
        "activitypub",
        Some("activitypub_object"),
        &id,
        &url,
    );
    row
}

fn dedupe_owner_public_search(results: &mut OwnerPublicSearch) {
    let mut seen_posts = HashSet::new();
    results.posts.retain(|row| {
        let key = string_field(Some(row), "id")
            .or_else(|| string_field(Some(row), "url"))
            .unwrap_or_default();
        !key.is_empty() && seen_posts.insert(key)
    });
    let mut seen_actors = HashSet::new();
    results.actors.retain(|row| {
        let key = string_field(Some(row), "id")
            .or_else(|| string_field(Some(row), "url"))
            .or_else(|| string_field(Some(row), "handle"))
            .unwrap_or_default();
        !key.is_empty() && seen_actors.insert(key)
    });
}

pub(crate) fn owner_normalize_bluesky_post(value: &Value) -> Option<Map<String, Value>> {
    let object = value.as_object()?;
    let uri = object.get("uri").and_then(optional_body_string)?;
    let author = object.get("author").and_then(Value::as_object);
    let handle = author
        .and_then(|row| row.get("handle"))
        .and_then(optional_body_string);
    let actor_id = author
        .and_then(|row| row.get("did"))
        .and_then(optional_body_string);
    let display_name = author
        .and_then(|row| row.get("displayName"))
        .and_then(optional_body_string);
    let record = object.get("record").and_then(Value::as_object);
    let text = record
        .and_then(|row| row.get("text"))
        .and_then(optional_body_string)
        .unwrap_or_default();
    let published_at = record
        .and_then(|row| row.get("createdAt"))
        .and_then(optional_body_string)
        .or_else(|| object.get("indexedAt").and_then(optional_body_string));
    let url = bluesky_post_url(&uri, handle.as_deref()).unwrap_or_else(|| uri.clone());
    let mut row = owner_public_post_row(OwnerPublicPostFields {
        provider: "bluesky".to_string(),
        network: "atproto".to_string(),
        id: uri.clone(),
        url: url.clone(),
        actor_id,
        actor_handle: handle,
        actor_display_name: display_name,
        content: text,
        content_html: None,
        summary: None,
        object_type: Some("app.bsky.feed.post".to_string()),
        published_at,
    });
    insert_optional_string(
        &mut row,
        "cid",
        object.get("cid").and_then(optional_body_string),
    );
    insert_optional_number(&mut row, "reply_count", object.get("replyCount"));
    insert_optional_number(&mut row, "repost_count", object.get("repostCount"));
    insert_optional_number(&mut row, "like_count", object.get("likeCount"));
    owner_add_public_post_actions(&mut row, "atproto", Some("bluesky_post"), &uri, &url);
    Some(row)
}

fn owner_normalize_bluesky_actor(value: &Value) -> Option<Map<String, Value>> {
    let object = value.as_object()?;
    let did = object.get("did").and_then(optional_body_string)?;
    let handle = object.get("handle").and_then(optional_body_string);
    let url = handle
        .as_ref()
        .map(|handle| format!("https://bsky.app/profile/{handle}"))
        .unwrap_or_else(|| did.clone());
    let mut row = owner_public_actor_row(
        "bluesky",
        "atproto",
        &did,
        handle.clone(),
        object.get("displayName").and_then(optional_body_string),
        object.get("description").and_then(optional_body_string),
        Some(url),
        object.get("avatar").and_then(optional_body_string),
    );
    insert_optional_string(&mut row, "watch_type", Some("bluesky_actor".to_string()));
    insert_optional_string(&mut row, "watch_target", handle.or_else(|| Some(did)));
    row.insert(
        "actions".to_string(),
        Value::Array(vec![
            Value::String("watch".to_string()),
            Value::String("open".to_string()),
        ]),
    );
    Some(row)
}

fn owner_normalize_mastodon_status(provider: &str, value: &Value) -> Option<Map<String, Value>> {
    let object = value.as_object()?;
    let id = object
        .get("uri")
        .or_else(|| object.get("url"))
        .or_else(|| object.get("id"))
        .and_then(optional_body_string)?;
    let url = object
        .get("url")
        .and_then(optional_body_string)
        .unwrap_or_else(|| id.clone());
    let content_html = object
        .get("content")
        .and_then(optional_body_string)
        .unwrap_or_default();
    let summary = object.get("spoiler_text").and_then(optional_body_string);
    let mut content = strip_html(&content_html);
    if content.is_empty() {
        content = summary.clone().unwrap_or_default();
    }
    let account = object.get("account").and_then(Value::as_object);
    let mut row = owner_public_post_row(OwnerPublicPostFields {
        provider: provider.to_string(),
        network: "activitypub".to_string(),
        id: id.clone(),
        url: url.clone(),
        actor_id: account
            .and_then(|row| row.get("url"))
            .and_then(optional_body_string),
        actor_handle: account
            .and_then(|row| row.get("acct"))
            .and_then(optional_body_string)
            .or_else(|| {
                account
                    .and_then(|row| row.get("username"))
                    .and_then(optional_body_string)
            }),
        actor_display_name: account
            .and_then(|row| row.get("display_name"))
            .and_then(optional_body_string),
        content,
        content_html: (!content_html.is_empty()).then_some(content_html),
        summary,
        object_type: Some("Note".to_string()),
        published_at: object.get("created_at").and_then(optional_body_string),
    });
    owner_add_public_post_actions(
        &mut row,
        "activitypub",
        Some("activitypub_object"),
        &id,
        &url,
    );
    Some(row)
}

fn owner_normalize_mastodon_account(provider: &str, value: &Value) -> Option<Map<String, Value>> {
    let object = value.as_object()?;
    let id = object
        .get("url")
        .or_else(|| object.get("uri"))
        .or_else(|| object.get("id"))
        .and_then(optional_body_string)?;
    let actor_url = object.get("url").and_then(optional_body_string);
    let mut row = owner_public_actor_row(
        provider,
        "activitypub",
        &id,
        object
            .get("acct")
            .and_then(optional_body_string)
            .or_else(|| object.get("username").and_then(optional_body_string)),
        object.get("display_name").and_then(optional_body_string),
        object
            .get("note")
            .and_then(optional_body_string)
            .map(|html| strip_html(&html)),
        actor_url.clone(),
        object.get("avatar").and_then(optional_body_string),
    );
    insert_optional_string(
        &mut row,
        "watch_type",
        Some("activitypub_actor".to_string()),
    );
    insert_optional_string(
        &mut row,
        "watch_target",
        actor_url.clone().or_else(|| Some(id.clone())),
    );
    insert_optional_string(&mut row, "follow_target", actor_url.or_else(|| Some(id)));
    row.insert(
        "actions".to_string(),
        Value::Array(vec![
            Value::String("watch".to_string()),
            Value::String("follow".to_string()),
            Value::String("open".to_string()),
        ]),
    );
    Some(row)
}

pub(crate) fn owner_normalize_tootfinder_status(value: Value) -> Option<Map<String, Value>> {
    let object = value.as_object()?;
    let id = object
        .get("uri")
        .or_else(|| object.get("url"))
        .or_else(|| object.get("id"))
        .and_then(optional_body_string)?;
    let url = object
        .get("url")
        .and_then(optional_body_string)
        .unwrap_or_else(|| id.clone());
    let content_html = object
        .get("content")
        .and_then(optional_body_string)
        .unwrap_or_default();
    let summary = object
        .get("spoiler_text")
        .or_else(|| object.get("spoiler"))
        .and_then(optional_body_string);
    let mut content = strip_html(&content_html);
    if content.is_empty() {
        content = summary.clone().unwrap_or_default();
    }
    let actor_id = activitypub_actor_id_from_status_uri(&id).or_else(|| {
        object
            .get("uri")
            .and_then(optional_body_string)
            .and_then(|uri| activitypub_actor_id_from_status_uri(&uri))
    });
    let actor_handle = actor_handle_from_public_status_url(&url)
        .or_else(|| actor_id.as_deref().and_then(actor_handle_from_actor_url));
    let mut row = owner_public_post_row(OwnerPublicPostFields {
        provider: "tootfinder.ch".to_string(),
        network: "activitypub".to_string(),
        id: id.clone(),
        url: url.clone(),
        actor_id,
        actor_handle,
        actor_display_name: None,
        content,
        content_html: (!content_html.is_empty()).then_some(content_html),
        summary,
        object_type: Some("Note".to_string()),
        published_at: object.get("created_at").and_then(optional_body_string),
    });
    owner_add_public_post_actions(
        &mut row,
        "activitypub",
        Some("activitypub_object"),
        &id,
        &url,
    );
    insert_optional_string(
        &mut row,
        "language",
        object.get("language").and_then(optional_body_string),
    );
    Some(row)
}

fn activitypub_actor_id_from_status_uri(uri: &str) -> Option<String> {
    let trimmed = uri.trim();
    let (actor, _) = trimmed.split_once("/statuses/")?;
    (!actor.is_empty()).then(|| actor.to_string())
}

fn actor_handle_from_public_status_url(url: &str) -> Option<String> {
    let parsed = worker::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let path = parsed.path();
    let username = path
        .strip_prefix("/@")
        .and_then(|rest| rest.split('/').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(format!("{username}@{host}"))
}

fn actor_handle_from_actor_url(url: &str) -> Option<String> {
    let parsed = worker::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let path = parsed.path();
    let username = path
        .strip_prefix("/users/")
        .or_else(|| path.strip_prefix("/@"))
        .and_then(|rest| rest.split('/').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(format!("{username}@{host}"))
}

struct OwnerPublicPostFields {
    provider: String,
    network: String,
    id: String,
    url: String,
    actor_id: Option<String>,
    actor_handle: Option<String>,
    actor_display_name: Option<String>,
    content: String,
    content_html: Option<String>,
    summary: Option<String>,
    object_type: Option<String>,
    published_at: Option<String>,
}

fn owner_public_post_row(fields: OwnerPublicPostFields) -> Map<String, Value> {
    let mut row = Map::new();
    row.insert("provider".to_string(), Value::String(fields.provider));
    row.insert("network".to_string(), Value::String(fields.network));
    row.insert("id".to_string(), Value::String(fields.id));
    row.insert("url".to_string(), Value::String(fields.url));
    row.insert("content".to_string(), Value::String(fields.content));
    insert_optional_string(&mut row, "actor_id", fields.actor_id);
    insert_optional_string(&mut row, "actor_handle", fields.actor_handle);
    insert_optional_string(&mut row, "actor_display_name", fields.actor_display_name);
    insert_optional_string(&mut row, "content_html", fields.content_html);
    insert_optional_string(&mut row, "summary", fields.summary);
    insert_optional_string(&mut row, "object_type", fields.object_type);
    insert_optional_string(&mut row, "published_at", fields.published_at);
    row
}

fn owner_add_public_post_actions(
    row: &mut Map<String, Value>,
    network: &str,
    watch_type: Option<&str>,
    id: &str,
    url: &str,
) {
    if let Some(watch_type) = watch_type {
        insert_optional_string(row, "watch_type", Some(watch_type.to_string()));
        insert_optional_string(row, "watch_target", Some(id.to_string()));
    }
    insert_optional_string(row, "reply_target", Some(id.to_string()));
    let mut actions = vec![
        Value::String("open".to_string()),
        Value::String("watch".to_string()),
        Value::String("reply".to_string()),
    ];
    if network == "activitypub" {
        actions.push(Value::String("like".to_string()));
        actions.push(Value::String("boost".to_string()));
    }
    row.insert("actions".to_string(), Value::Array(actions));
    if !url.is_empty() {
        insert_optional_string(row, "canonical_url", Some(url.to_string()));
    }
}

fn owner_public_actor_row(
    provider: &str,
    network: &str,
    id: &str,
    handle: Option<String>,
    display_name: Option<String>,
    summary: Option<String>,
    url: Option<String>,
    avatar_url: Option<String>,
) -> Map<String, Value> {
    let mut row = Map::new();
    row.insert("provider".to_string(), Value::String(provider.to_string()));
    row.insert("network".to_string(), Value::String(network.to_string()));
    row.insert("id".to_string(), Value::String(id.to_string()));
    insert_optional_string(&mut row, "handle", handle);
    insert_optional_string(&mut row, "display_name", display_name);
    insert_optional_string(&mut row, "summary", summary);
    insert_optional_string(&mut row, "url", url);
    insert_optional_string(&mut row, "avatar_url", avatar_url);
    row
}

fn owner_search_provider_error(provider: &str, network: &str, error: &str) -> Map<String, Value> {
    let mut row = Map::new();
    row.insert("provider".to_string(), Value::String(provider.to_string()));
    row.insert("network".to_string(), Value::String(network.to_string()));
    row.insert("error".to_string(), Value::String(error.to_string()));
    row
}

pub(crate) fn bluesky_post_url(uri: &str, handle: Option<&str>) -> Option<String> {
    let handle = handle?.trim();
    let rkey = uri.rsplit('/').next()?.trim();
    if handle.is_empty() || rkey.is_empty() {
        return None;
    }
    Some(format!("https://bsky.app/profile/{handle}/post/{rkey}"))
}

fn insert_optional_string(row: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        row.insert(key.to_string(), Value::String(value));
    }
}

fn insert_optional_number(row: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    if let Some(Value::Number(number)) = value {
        row.insert(key.to_string(), Value::Number(number.clone()));
    }
}
