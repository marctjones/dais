use worker::Env;

pub(crate) fn env_string(env: &Env, name: &str, fallback: &str) -> String {
    env.var(name)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| fallback.to_string())
}

pub(crate) fn local_username(env: &Env) -> String {
    env_string(env, "USERNAME", "social")
}

pub(crate) fn handle_domain(env: &Env) -> String {
    env_string(env, "DOMAIN", "dais.social")
}

pub(crate) fn activitypub_domain(env: &Env) -> String {
    env.var("ACTIVITYPUB_DOMAIN")
        .map(|value| value.to_string())
        .unwrap_or_else(|_| format!("social.{}", handle_domain(env)))
}

pub(crate) fn local_actor_url(env: &Env) -> String {
    format!(
        "https://{}/users/{}",
        activitypub_domain(env),
        local_username(env)
    )
}

pub(crate) fn local_actor_url_for_request(env: &Env, url: &worker::Url) -> String {
    format!("{}/users/{}", origin(url), local_username(env))
}

pub(crate) fn activitypub_user_prefix(env: &Env) -> String {
    format!("/users/{}", local_username(env))
}

pub(crate) fn origin(url: &worker::Url) -> String {
    format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default())
}

pub(crate) fn owner_instance_url(env: &Env) -> String {
    let domain = env
        .var("DOMAIN")
        .map(|value| value.to_string())
        .unwrap_or_else(|_| "social.dais.social".to_string());
    if domain.starts_with("http://") || domain.starts_with("https://") {
        domain.trim_end_matches('/').to_string()
    } else {
        format!("https://{}", domain.trim_end_matches('/'))
    }
}
