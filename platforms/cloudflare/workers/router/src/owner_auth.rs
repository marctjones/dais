use serde_json::Value;
use worker::Env;

pub(crate) struct OwnerToken {
    pub(crate) token: String,
    pub(crate) scopes: Vec<String>,
}

pub(crate) fn owner_bearer_tokens(env: &Env) -> Vec<OwnerToken> {
    let mut tokens = Vec::new();
    let configured = env
        .secret("OWNER_API_TOKEN")
        .map(|value| value.to_string())
        .or_else(|_| {
            env.secret("DAIS_OWNER_TOKEN")
                .map(|value| value.to_string())
        })
        .or_else(|_| env.var("OWNER_API_TOKEN").map(|value| value.to_string()))
        .or_else(|_| env.var("DAIS_OWNER_TOKEN").map(|value| value.to_string()))
        .unwrap_or_else(|_| {
            if remote_environment(env) {
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

pub(crate) fn remote_environment(env: &Env) -> bool {
    env.var("ENVIRONMENT")
        .map(|value| value.to_string() != "dev")
        .unwrap_or(false)
}

fn scoped_owner_tokens(env: &Env) -> Vec<OwnerToken> {
    let mut tokens = Vec::new();
    for raw in [
        optional_env_secret_or_var(env, "OWNER_API_SCOPED_TOKENS"),
        optional_env_secret_or_var(env, "DAIS_OWNER_SCOPED_TOKENS"),
    ]
    .into_iter()
    .flatten()
    {
        tokens.extend(parse_scoped_owner_tokens(&raw));
    }
    tokens
}

fn optional_env_secret_or_var(env: &Env, name: &str) -> Option<String> {
    env.secret(name)
        .map(|value| value.to_string())
        .or_else(|_| env.var(name).map(|value| value.to_string()))
        .ok()
}

pub(crate) fn parse_scoped_owner_tokens(raw: &str) -> Vec<OwnerToken> {
    if raw.trim().is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<Value>(raw) {
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

pub(crate) fn owner_token_has_scopes(scopes: &[String], required_scopes: &[&str]) -> bool {
    scopes
        .iter()
        .any(|scope| scope == "owner" || scope == "admin" || scope == "*")
        || required_scopes
            .iter()
            .all(|required| scopes.iter().any(|scope| scope == required))
}
