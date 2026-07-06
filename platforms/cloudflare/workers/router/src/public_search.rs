use crate::normalize_host_value;
use crate::request::query_param;
use serde_json::Value;

const BLUESKY_APPVIEW_BASE_URL: &str = "https://api.bsky.app";
const TOOTFINDER_SEARCH_BASE_URL: &str = "https://www.tootfinder.ch/rest/api/search";
pub(crate) const MAX_ACTIVITYPUB_SEARCH_SERVERS: usize = 5;

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
