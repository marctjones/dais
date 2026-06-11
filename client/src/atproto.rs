use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::BlueskyConfig;

#[derive(Debug)]
pub struct AtprotoClient {
    http: Client,
    service: String,
    appview: String,
    handle: String,
    password: String,
    did: String,
    access_jwt: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Session {
    pub did: String,
    pub handle: Option<String>,
    #[serde(rename = "accessJwt", alias = "access_jwt")]
    pub access_jwt: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreatedRecord {
    pub uri: String,
    pub cid: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Profile {
    pub did: String,
    pub handle: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "followersCount")]
    pub followers_count: Option<u64>,
    #[serde(rename = "followsCount")]
    pub follows_count: Option<u64>,
    #[serde(rename = "postsCount")]
    pub posts_count: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FeedResponse {
    pub feed: Vec<FeedItem>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FeedItem {
    pub post: FeedPost,
    pub reason: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FeedPost {
    pub uri: String,
    pub cid: Option<String>,
    pub author: Profile,
    pub record: FeedRecord,
    #[serde(rename = "replyCount")]
    pub reply_count: Option<u64>,
    #[serde(rename = "repostCount")]
    pub repost_count: Option<u64>,
    #[serde(rename = "likeCount")]
    pub like_count: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FeedRecord {
    pub text: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FollowsResponse {
    follows: Vec<Profile>,
}

#[derive(Debug, Deserialize)]
struct FollowersResponse {
    followers: Vec<Profile>,
}

#[derive(Debug, Deserialize)]
struct RecordsResponse {
    records: Vec<RecordView>,
}

#[derive(Debug, Deserialize)]
struct RecordView {
    uri: String,
    value: RecordValue,
}

#[derive(Debug, Deserialize)]
struct RecordValue {
    subject: Option<RecordSubject>,
    #[serde(rename = "$type")]
    record_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RecordSubject {
    Did(String),
    Ref {
        uri: String,
        #[serde(rename = "cid")]
        _cid: Option<String>,
    },
}

#[derive(Debug, Serialize)]
struct PostRecord<'a> {
    #[serde(rename = "$type")]
    record_type: &'static str,
    text: &'a str,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply: Option<ReplyRef<'a>>,
}

#[derive(Debug, Serialize)]
struct ReplyRef<'a> {
    root: StrongRef<'a>,
    parent: StrongRef<'a>,
}

#[derive(Debug, Serialize)]
struct StrongRef<'a> {
    uri: &'a str,
    cid: &'a str,
}

impl AtprotoClient {
    pub fn new(service: String, appview: String) -> Result<Self> {
        Ok(Self {
            http: Client::builder().user_agent("dais-client/0.1").build()?,
            service,
            appview,
            handle: String::new(),
            password: String::new(),
            did: String::new(),
            access_jwt: None,
        })
    }

    pub fn from_config(config: &BlueskyConfig) -> Result<Self> {
        Ok(Self {
            http: Client::builder().user_agent("dais-client/0.1").build()?,
            service: config.service.trim_end_matches('/').to_string(),
            appview: config.appview.trim_end_matches('/').to_string(),
            handle: config.handle.clone(),
            password: config.password.clone(),
            did: config.did.clone(),
            access_jwt: None,
        })
    }

    pub fn handle(&self) -> &str {
        &self.handle
    }

    pub fn did(&self) -> &str {
        &self.did
    }

    pub async fn create_session(&mut self, identifier: &str, password: &str) -> Result<Session> {
        let session: Session = self
            .post_json(
                &self.service,
                "com.atproto.server.createSession",
                json!({
                    "identifier": identifier,
                    "password": password,
                }),
                None,
            )
            .await?;

        self.handle = session
            .handle
            .clone()
            .unwrap_or_else(|| identifier.to_string());
        self.password = password.to_string();
        self.did = session.did.clone();
        self.access_jwt = Some(session.access_jwt.clone());

        Ok(session)
    }

    pub async fn ensure_session(&mut self) -> Result<()> {
        if self.access_jwt.is_none() {
            let handle = self.handle.clone();
            let password = self.password.clone();
            self.create_session(&handle, &password).await?;
        }
        Ok(())
    }

    pub async fn create_post(&mut self, text: &str) -> Result<CreatedRecord> {
        self.ensure_session().await?;
        let token = self.token()?;
        let record = PostRecord {
            record_type: "app.bsky.feed.post",
            text,
            created_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            reply: None,
        };

        self.post_json(
            &self.service,
            "com.atproto.repo.createRecord",
            json!({
                "repo": self.did,
                "collection": "app.bsky.feed.post",
                "record": record,
            }),
            Some(token),
        )
        .await
    }

    pub async fn reply_post(
        &mut self,
        text: &str,
        parent_uri: &str,
        parent_cid: &str,
        root_uri: &str,
        root_cid: &str,
    ) -> Result<CreatedRecord> {
        self.ensure_session().await?;
        let token = self.token()?;
        let record = PostRecord {
            record_type: "app.bsky.feed.post",
            text,
            created_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            reply: Some(ReplyRef {
                root: StrongRef {
                    uri: root_uri,
                    cid: root_cid,
                },
                parent: StrongRef {
                    uri: parent_uri,
                    cid: parent_cid,
                },
            }),
        };

        self.post_json(
            &self.service,
            "com.atproto.repo.createRecord",
            json!({
                "repo": self.did,
                "collection": "app.bsky.feed.post",
                "record": record,
            }),
            Some(token),
        )
        .await
    }

    pub async fn like(&mut self, uri: &str, cid: &str) -> Result<CreatedRecord> {
        self.create_subject_record("app.bsky.feed.like", uri, cid)
            .await
    }

    pub async fn unlike(&mut self, uri: &str) -> Result<()> {
        self.delete_subject_record("app.bsky.feed.like", uri).await
    }

    pub async fn repost(&mut self, uri: &str, cid: &str) -> Result<CreatedRecord> {
        self.create_subject_record("app.bsky.feed.repost", uri, cid)
            .await
    }

    pub async fn unrepost(&mut self, uri: &str) -> Result<()> {
        self.delete_subject_record("app.bsky.feed.repost", uri).await
    }

    pub async fn get_profile(&mut self, actor: &str) -> Result<Profile> {
        self.ensure_session().await?;
        self.get_json(
            &self.appview,
            "app.bsky.actor.getProfile",
            &[("actor", actor)],
        )
        .await
    }

    pub async fn get_author_feed(&mut self, actor: &str, limit: u16) -> Result<FeedResponse> {
        self.ensure_session().await?;
        let limit = limit.to_string();
        self.get_json(
            &self.appview,
            "app.bsky.feed.getAuthorFeed",
            &[("actor", actor), ("limit", &limit)],
        )
        .await
    }

    pub async fn get_timeline(&mut self, limit: u16) -> Result<FeedResponse> {
        self.ensure_session().await?;
        let token = self.token()?;
        let limit = limit.to_string();
        self.get_json_auth(
            &self.appview,
            "app.bsky.feed.getTimeline",
            &[("limit", &limit)],
            Some(token),
        )
        .await
    }

    pub async fn follow(&mut self, subject_did: &str) -> Result<CreatedRecord> {
        self.ensure_session().await?;
        let token = self.token()?;

        self.post_json(
            &self.service,
            "com.atproto.repo.createRecord",
            json!({
                "repo": self.did,
                "collection": "app.bsky.graph.follow",
                "record": {
                    "$type": "app.bsky.graph.follow",
                    "subject": subject_did,
                    "createdAt": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                },
            }),
            Some(token),
        )
        .await
    }

    pub async fn unfollow(&mut self, subject_did: &str) -> Result<()> {
        self.ensure_session().await?;
        let token = self.token()?;
        let records: RecordsResponse = self
            .get_json_auth(
                &self.service,
                "com.atproto.repo.listRecords",
                &[
                    ("repo", self.did.as_str()),
                    ("collection", "app.bsky.graph.follow"),
                    ("limit", "100"),
                ],
                Some(token),
            )
            .await?;

        let follow_uri = records
            .records
            .into_iter()
            .find(|record| match record.value.subject.as_ref() {
                Some(RecordSubject::Did(did)) => did == subject_did,
                _ => false,
            })
            .map(|record| record.uri)
            .ok_or_else(|| anyhow!("not following DID {subject_did}"))?;
        let rkey = follow_uri
            .rsplit('/')
            .next()
            .filter(|part| !part.is_empty())
            .ok_or_else(|| anyhow!("could not extract follow rkey from {follow_uri}"))?;

        let _: serde_json::Value = self
            .post_json(
                &self.service,
                "com.atproto.repo.deleteRecord",
                json!({
                    "repo": self.did,
                    "collection": "app.bsky.graph.follow",
                    "rkey": rkey,
                }),
                Some(token),
            )
            .await?;

        Ok(())
    }

    async fn create_subject_record(
        &mut self,
        collection: &str,
        uri: &str,
        cid: &str,
    ) -> Result<CreatedRecord> {
        self.ensure_session().await?;
        let token = self.token()?;

        self.post_json(
            &self.service,
            "com.atproto.repo.createRecord",
            json!({
                "repo": self.did,
                "collection": collection,
                "record": {
                    "$type": collection,
                    "subject": {
                        "uri": uri,
                        "cid": cid,
                    },
                    "createdAt": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                },
            }),
            Some(token),
        )
        .await
    }

    async fn delete_subject_record(&mut self, collection: &str, uri: &str) -> Result<()> {
        self.ensure_session().await?;
        let token = self.token()?;
        let records: RecordsResponse = self
            .get_json_auth(
                &self.service,
                "com.atproto.repo.listRecords",
                &[
                    ("repo", self.did.as_str()),
                    ("collection", collection),
                    ("limit", "100"),
                ],
                Some(token),
            )
            .await?;

        let record_uri = records
            .records
            .into_iter()
            .find(|record| {
                record.value.record_type.as_deref() == Some(collection)
                    && record
                        .value
                        .subject
                        .as_ref()
                        .is_some_and(|subject| match subject {
                            RecordSubject::Ref {
                                uri: subject_uri, ..
                            } => subject_uri == uri,
                            RecordSubject::Did(_) => false,
                        })
            })
            .map(|record| record.uri)
            .ok_or_else(|| anyhow!("no {collection} record found for {uri}"))?;
        let rkey = record_uri
            .rsplit('/')
            .next()
            .filter(|part| !part.is_empty())
            .ok_or_else(|| anyhow!("could not extract rkey from {record_uri}"))?;

        let _: serde_json::Value = self
            .post_json(
                &self.service,
                "com.atproto.repo.deleteRecord",
                json!({
                    "repo": self.did,
                    "collection": collection,
                    "rkey": rkey,
                }),
                Some(token),
            )
            .await?;

        Ok(())
    }

    pub async fn get_follows(&mut self, actor: &str, limit: u16) -> Result<Vec<Profile>> {
        self.ensure_session().await?;
        let limit = limit.to_string();
        let response: FollowsResponse = self
            .get_json(
                &self.appview,
                "app.bsky.graph.getFollows",
                &[("actor", actor), ("limit", &limit)],
            )
            .await?;
        Ok(response.follows)
    }

    pub async fn get_followers(&mut self, actor: &str, limit: u16) -> Result<Vec<Profile>> {
        self.ensure_session().await?;
        let limit = limit.to_string();
        let response: FollowersResponse = self
            .get_json(
                &self.appview,
                "app.bsky.graph.getFollowers",
                &[("actor", actor), ("limit", &limit)],
            )
            .await?;
        Ok(response.followers)
    }

    fn token(&self) -> Result<&str> {
        self.access_jwt
            .as_deref()
            .ok_or_else(|| anyhow!("ATProto session is not authenticated"))
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        base: &str,
        method: &str,
        query: &[(&str, &str)],
    ) -> Result<T> {
        self.get_json_auth(base, method, query, None).await
    }

    async fn get_json_auth<T: for<'de> Deserialize<'de>>(
        &self,
        base: &str,
        method: &str,
        query: &[(&str, &str)],
        bearer: Option<&str>,
    ) -> Result<T> {
        let url = format!("{}/xrpc/{}", base.trim_end_matches('/'), method);
        let mut request = self.http.get(url).query(query);
        if let Some(token) = bearer {
            request = request.bearer_auth(token);
        }

        let response = request.send().await?;
        decode_response(response).await
    }

    async fn post_json<T: for<'de> Deserialize<'de>>(
        &self,
        base: &str,
        method: &str,
        body: serde_json::Value,
        bearer: Option<&str>,
    ) -> Result<T> {
        let url = format!("{}/xrpc/{}", base.trim_end_matches('/'), method);
        let mut request = self.http.post(url).json(&body);
        if let Some(token) = bearer {
            request = request.bearer_auth(token);
        }

        let response = request.send().await?;
        decode_response(response).await
    }
}

async fn decode_response<T: for<'de> Deserialize<'de>>(response: reqwest::Response) -> Result<T> {
    let status = response.status();
    let text = response.text().await?;
    if status != StatusCode::OK {
        return Err(anyhow!("ATProto request failed with {status}: {text}"));
    }

    serde_json::from_str(&text).with_context(|| format!("could not parse ATProto response: {text}"))
}
