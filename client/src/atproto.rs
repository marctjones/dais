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

#[derive(Clone, Debug)]
pub struct PostRef {
    pub uri: String,
    pub cid: String,
}

#[derive(Clone, Debug)]
pub struct ReplyTarget {
    pub root: PostRef,
    pub parent: PostRef,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProfileRecord {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    embed: Option<ImageEmbed>,
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

#[derive(Debug, Serialize)]
struct ImageEmbed {
    #[serde(rename = "$type")]
    record_type: &'static str,
    images: Vec<ImageEmbedItem>,
}

#[derive(Debug, Serialize)]
struct ImageEmbedItem {
    alt: String,
    image: BlobRef,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BlobRef {
    #[serde(rename = "$type")]
    pub blob_type: Option<String>,
    #[serde(rename = "ref")]
    pub ref_: BlobLink,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub size: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BlobLink {
    #[serde(rename = "$link")]
    pub link: String,
}

#[derive(Debug, Deserialize)]
struct UploadBlobResponse {
    blob: BlobRef,
}

#[derive(Debug, Deserialize)]
struct GetRecordResponse {
    uri: String,
    cid: Option<String>,
    value: GetRecordValue,
}

#[derive(Debug, Deserialize)]
struct GetRecordValue {
    reply: Option<GetRecordReply>,
}

#[derive(Debug, Deserialize)]
struct GetRecordReply {
    root: GetRecordStrongRef,
}

#[derive(Debug, Deserialize)]
struct GetRecordStrongRef {
    uri: String,
    cid: String,
}

#[derive(Debug)]
pub struct ImageUpload {
    pub blob: BlobRef,
    pub alt: String,
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
        self.create_post_with_images(text, Vec::new()).await
    }

    pub async fn create_post_with_images(
        &mut self,
        text: &str,
        images: Vec<ImageUpload>,
    ) -> Result<CreatedRecord> {
        self.create_post_record(text, None, images).await
    }

    pub async fn create_reply_with_images(
        &mut self,
        text: &str,
        reply: ReplyTarget,
        images: Vec<ImageUpload>,
    ) -> Result<CreatedRecord> {
        self.create_post_record(text, Some(reply), images).await
    }

    async fn create_post_record(
        &mut self,
        text: &str,
        reply: Option<ReplyTarget>,
        images: Vec<ImageUpload>,
    ) -> Result<CreatedRecord> {
        self.ensure_session().await?;
        let token = self.token()?;
        let reply = reply.as_ref().map(|target| ReplyRef {
            root: StrongRef {
                uri: &target.root.uri,
                cid: &target.root.cid,
            },
            parent: StrongRef {
                uri: &target.parent.uri,
                cid: &target.parent.cid,
            },
        });
        let record = PostRecord {
            record_type: "app.bsky.feed.post",
            text,
            created_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            reply,
            embed: image_embed(images),
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

    pub async fn upload_blob(&mut self, bytes: Vec<u8>, content_type: &str) -> Result<BlobRef> {
        self.ensure_session().await?;
        let token = self.token()?;
        let url = format!("{}/xrpc/com.atproto.repo.uploadBlob", self.service);
        let response = self
            .http
            .post(url)
            .bearer_auth(token)
            .header("Content-Type", content_type)
            .body(bytes)
            .send()
            .await?;
        let uploaded: UploadBlobResponse = decode_response(response).await?;
        Ok(uploaded.blob)
    }

    pub async fn resolve_reply_target(&mut self, uri: &str) -> Result<ReplyTarget> {
        let (repo, collection, rkey) = parse_at_uri(uri)?;
        if collection != "app.bsky.feed.post" {
            return Err(anyhow!(
                "ATProto replies currently require an app.bsky.feed.post URI"
            ));
        }
        let record: GetRecordResponse = self
            .get_json(
                &self.service,
                "com.atproto.repo.getRecord",
                &[
                    ("repo", repo.as_str()),
                    ("collection", collection),
                    ("rkey", rkey.as_str()),
                ],
            )
            .await?;
        let parent = PostRef {
            uri: record.uri,
            cid: record
                .cid
                .ok_or_else(|| anyhow!("reply target record is missing cid"))?,
        };
        let root = record
            .value
            .reply
            .map(|reply| PostRef {
                uri: reply.root.uri,
                cid: reply.root.cid,
            })
            .unwrap_or_else(|| parent.clone());
        Ok(ReplyTarget { root, parent })
    }

    pub async fn reply_post(
        &mut self,
        text: &str,
        parent_uri: &str,
        parent_cid: &str,
        root_uri: &str,
        root_cid: &str,
    ) -> Result<CreatedRecord> {
        self.create_reply_with_images(
            text,
            ReplyTarget {
                root: PostRef {
                    uri: root_uri.to_string(),
                    cid: root_cid.to_string(),
                },
                parent: PostRef {
                    uri: parent_uri.to_string(),
                    cid: parent_cid.to_string(),
                },
            },
            Vec::new(),
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
        self.delete_subject_record("app.bsky.feed.repost", uri)
            .await
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

    pub async fn update_profile_record(
        &mut self,
        display_name: Option<&str>,
        description: Option<&str>,
    ) -> Result<ProfileRecord> {
        if display_name.is_none() && description.is_none() {
            return Err(anyhow!("no profile fields provided"));
        }
        self.ensure_session().await?;
        let token = self.token()?;
        let mut record = serde_json::Map::new();
        record.insert(
            "$type".to_string(),
            serde_json::Value::String("app.bsky.actor.profile".to_string()),
        );
        if let Some(value) = display_name {
            record.insert(
                "displayName".to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
        if let Some(value) = description {
            record.insert(
                "description".to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
        self.post_json(
            &self.service,
            "com.atproto.repo.createRecord",
            json!({
                "repo": self.did,
                "collection": "app.bsky.actor.profile",
                "rkey": "self",
                "record": record,
            }),
            Some(token),
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

fn image_embed(images: Vec<ImageUpload>) -> Option<ImageEmbed> {
    if images.is_empty() {
        return None;
    }
    Some(ImageEmbed {
        record_type: "app.bsky.embed.images",
        images: images
            .into_iter()
            .map(|image| ImageEmbedItem {
                alt: image.alt,
                image: image.blob,
            })
            .collect(),
    })
}

fn parse_at_uri(uri: &str) -> Result<(String, &str, String)> {
    let rest = uri
        .strip_prefix("at://")
        .ok_or_else(|| anyhow!("ATProto reply target must be an at:// URI"))?;
    let mut parts = rest.split('/');
    let repo = parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("ATProto URI missing repo"))?
        .to_string();
    let collection = parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("ATProto URI missing collection"))?;
    let rkey = parts
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("ATProto URI missing record key"))?
        .to_string();
    if parts.next().is_some() {
        return Err(anyhow!("ATProto URI has too many path segments"));
    }
    Ok((repo, collection, rkey))
}

async fn decode_response<T: for<'de> Deserialize<'de>>(response: reqwest::Response) -> Result<T> {
    let status = response.status();
    let text = response.text().await?;
    if status != StatusCode::OK {
        return Err(anyhow!("ATProto request failed with {status}: {text}"));
    }

    serde_json::from_str(&text).with_context(|| format!("could not parse ATProto response: {text}"))
}
