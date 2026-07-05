use serde::Serialize;
use worker::{Headers, Response, Result};

pub(crate) fn api_json<T: Serialize>(value: &T, status: u16) -> Result<Response> {
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
    headers.set("Cache-Control", "no-store")?;
    headers.set("Vary", "Authorization, Accept")?;
    let mut response = if status == 204 {
        Response::empty()?.with_status(status)
    } else {
        Response::from_json(value)?.with_status(status)
    };
    response = response.with_headers(headers);
    Ok(response)
}

pub(crate) fn text_response(body: &str, content_type: &str) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Content-Type", content_type)?;
    headers.set("Access-Control-Allow-Origin", "*")?;
    Ok(Response::ok(body.to_string())?.with_headers(headers))
}

pub(crate) fn activity_json<T: Serialize>(value: &T) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Content-Type", "application/activity+json; charset=utf-8")?;
    headers.set("Cache-Control", "no-store")?;
    Ok(Response::from_json(value)?.with_headers(headers))
}

pub(crate) fn jrd_json<T: Serialize>(value: &T, status: u16) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Content-Type", "application/jrd+json; charset=utf-8")?;
    headers.set("Access-Control-Allow-Origin", "*")?;
    Ok(Response::from_json(value)?
        .with_status(status)
        .with_headers(headers))
}

pub(crate) fn activitypub_error(message: &str, status: u16) -> Result<Response> {
    api_json(&serde_json::json!({ "error": message }), status)
}
