mod atproto;
mod cli;
mod config;
mod d1;
mod delivery;
mod doctor;
mod e2ee;
mod integrations;
mod output;
mod posting;
mod routing;
mod sources;
mod tui;

use anyhow::Result;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use clap::{CommandFactory, Parser};
use cli::{
    ActorsCommand, BlueskyCommand, Cli, Command, DeliveriesCommand, E2eeCommand, EventsCommand,
    FollowCommand, FollowersCommand, FriendsCommand, MediaCommand, ModerationCommand,
    NotificationsCommand, OwnerCommand, PostCommand, ReportsCommand, SearchCommand,
    TimelineCommand,
};
use config::ConfigStore;
use d1::D1Client;
use dais_client_core::{
    ComposeDraft as OwnerComposeDraft, DiagnosticStatus, ModerationState, OwnerApiClient,
    OwnerAudienceList, OwnerDelivery, OwnerDirectMessage, OwnerDiscoveredActor, OwnerE2eeDevice,
    OwnerE2eeDeviceRef, OwnerE2eeDeviceUpsert, OwnerE2eeMessage, OwnerE2eeMessageSend,
    OwnerE2eePeerDevice, OwnerE2eePeerDeviceRef, OwnerE2eePeerDiscoverRequest,
    OwnerE2eePeerTrustRequest, OwnerFollower, OwnerFollowing, OwnerFriend, OwnerInteraction,
    OwnerMediaUpload, OwnerNotification, OwnerPostDetail, OwnerProfile, OwnerProfileUpdate,
    OwnerSearchQuery, OwnerSearchResult, OwnerSnapshot, OwnerSourceAdd, OwnerSources, OwnerStats,
    OwnerWatchAdd, ProtocolRoute as OwnerProtocolRoute, Visibility as OwnerVisibility,
};
use posting::{
    delete_activitypub_post, publish_interaction, publish_post, update_activitypub_post,
    ActivityOutcome, PostDraft, PostOutcome,
};
use rand::rngs::OsRng;
use rand::RngCore;
use routing::{Protocol, Visibility};
use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::{RsaPrivateKey, RsaPublicKey};
use std::collections::BTreeMap;
use std::fs;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let store = ConfigStore::default()?;

    match cli.command {
        Command::Actors(command) => handle_actors(command).await?,
        Command::Bluesky(command) => handle_bluesky(command, &store).await?,
        Command::Post(command) => handle_post(command, &store).await?,
        Command::Search(command) => handle_search(command).await?,
        Command::Stats(args) => handle_stats(args).await?,
        Command::Timeline(command) => handle_timeline(command, &store).await?,
        Command::Friends(command) => handle_friends(command).await?,
        Command::Owner(command) => handle_owner(command, &store).await?,
        Command::Followers(command) => handle_followers(command).await?,
        Command::Notifications(command) => handle_notifications(command).await?,
        Command::Deliveries(command) => handle_deliveries(command).await?,
        Command::E2ee(command) => handle_e2ee(command).await?,
        Command::Events(command) => handle_events(command, &store).await?,
        Command::Media(command) => handle_media(command).await?,
        Command::Moderation(command) => handle_moderation(command).await?,
        Command::Reports(command) => handle_reports(command).await?,
        Command::Sources(command) => handle_sources(command).await?,
        Command::Doctor(args) => handle_doctor(args).await?,
        Command::Completions { shell } => {
            let mut command = Cli::command();
            let name = command.get_name().to_string();
            clap_complete::generate(shell, &mut command, name, &mut std::io::stdout());
        }
        Command::Tui(args) => tui::run(args.remote, &store).await?,
    }

    Ok(())
}

async fn handle_doctor(args: cli::DoctorArgs) -> Result<()> {
    let json = args.json;
    let report = doctor::run(&args).await;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        doctor::print_report(&report);
    }
    if report.has_failures() {
        std::process::exit(1);
    }
    Ok(())
}

async fn handle_actors(command: ActorsCommand) -> Result<()> {
    match command {
        ActorsCommand::Show { remote, username } => {
            let db = D1Client::new(remote)?;
            let actor = db
                .get_actor(&username)
                .await?
                .ok_or_else(|| anyhow::anyhow!("actor not found: {username}"))?;
            println!("Actor: {}", actor.id);
            println!("Username: {}", actor.username);
            println!(
                "Type: {}",
                actor.actor_type.unwrap_or_else(|| "Person".to_string())
            );
            if let Some(display_name) = actor.display_name {
                println!("Name: {display_name}");
            }
            if let Some(summary) = actor.summary {
                println!("Summary: {summary}");
            }
        }
        ActorsCommand::SetType {
            actor_type,
            remote,
            username,
        } => {
            let db = D1Client::new(remote)?;
            db.set_actor_type(&username, actor_type).await?;
            println!("@{username} actor type set to {actor_type}");
        }
        ActorsCommand::Update(args) => {
            let db = D1Client::new(args.remote)?;
            db.update_actor_profile(
                &args.username,
                args.display_name.as_deref(),
                args.summary.as_deref(),
                args.icon.as_deref(),
                args.image.as_deref(),
            )
            .await?;
            let actor = db
                .get_actor(&args.username)
                .await?
                .ok_or_else(|| anyhow::anyhow!("actor not found: {}", args.username))?;
            let mut object = serde_json::json!({
                "id": args.actor,
                "type": actor.actor_type.as_deref().unwrap_or("Person"),
                "preferredUsername": actor.username,
            });
            if let Some(display_name) = actor.display_name.as_deref() {
                object["name"] = serde_json::json!(display_name);
            }
            if let Some(summary) = actor.summary.as_deref() {
                object["summary"] = serde_json::json!(summary);
            }
            if let Some(icon) = actor.icon.as_deref() {
                object["icon"] = serde_json::json!({
                    "type": "Image",
                    "url": icon
                });
            }
            if let Some(image) = actor.image.as_deref() {
                object["image"] = serde_json::json!({
                    "type": "Image",
                    "url": image
                });
            }

            let now = chrono::Utc::now().to_rfc3339();
            let activity_id = format!("{}#updates/{}", args.actor, new_local_post_id());
            let activity_json = serde_json::json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "id": activity_id,
                "type": "Update",
                "actor": args.actor,
                "published": now,
                "to": ["https://www.w3.org/ns/activitystreams#Public"],
                "object": object
            })
            .to_string();
            let delivery_ids = db
                .create_activity_deliveries(d1::ActivityDeliveryInsert {
                    post_id: &args.actor,
                    actor_id: &args.actor,
                    activity_type: "Update",
                    activity_json: &activity_json,
                    target_inboxes: &[],
                })
                .await?;
            println!("@{} profile updated", args.username);
            println!("Activity: {activity_id}");
            println!("Deliveries queued: {}", delivery_ids.len());
        }
    }

    Ok(())
}

async fn handle_bluesky(command: BlueskyCommand, store: &ConfigStore) -> Result<()> {
    match command {
        BlueskyCommand::Login(args) => {
            let password = match args.password {
                Some(password) => password,
                None => prompt_password()?,
            };
            let service = args.service.trim_end_matches('/').to_string();
            let appview = args.appview.trim_end_matches('/').to_string();
            let mut client = atproto::AtprotoClient::new(service.clone(), appview.clone())?;
            let session = client.create_session(&args.handle, &password).await?;

            store.save_bluesky(&config::BlueskyConfig {
                handle: session.handle.clone().unwrap_or(args.handle),
                did: session.did,
                password,
                service,
                appview,
            })?;

            println!(
                "Logged in as {}",
                session
                    .handle
                    .unwrap_or_else(|| "configured account".to_string())
            );
        }
        BlueskyCommand::Logout => {
            store.delete_bluesky()?;
            println!("Logged out of Bluesky");
        }
        BlueskyCommand::Whoami => {
            let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;
            client.ensure_session().await?;
            let handle = client.handle().to_string();
            let profile = client.get_profile(&handle).await?;
            output::print_profile(&profile);
        }
        BlueskyCommand::Profile { handle } => {
            let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;
            client.ensure_session().await?;
            let profile = client.get_profile(handle.trim_start_matches('@')).await?;
            output::print_profile(&profile);
        }
        BlueskyCommand::UpdateProfile(args) => {
            let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;
            let updated = client
                .update_profile_record(args.display_name.as_deref(), args.description.as_deref())
                .await?;
            println!("Updated Bluesky profile record");
            println!("URI: {}", updated.uri);
            if let Some(cid) = updated.cid {
                println!("CID: {cid}");
            }
        }
        BlueskyCommand::Post(command) => handle_bluesky_post(command, store).await?,
        BlueskyCommand::Timeline(command) => handle_bluesky_timeline(command, store).await?,
        BlueskyCommand::Follow(command) => handle_bluesky_follow(command, store).await?,
    }

    Ok(())
}

async fn handle_bluesky_post(command: PostCommand, store: &ConfigStore) -> Result<()> {
    match command {
        PostCommand::Create { text } => {
            let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;
            let created = client.create_post(&text).await?;
            println!("Posted to Bluesky");
            println!("URI: {}", created.uri);
            if let Some(cid) = created.cid {
                println!("CID: {cid}");
            }
        }
        PostCommand::Reply {
            text,
            uri,
            cid,
            root_uri,
            root_cid,
        } => {
            let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;
            let root_uri = root_uri.unwrap_or_else(|| uri.clone());
            let root_cid = root_cid.unwrap_or_else(|| cid.clone());
            let created = client
                .reply_post(&text, &uri, &cid, &root_uri, &root_cid)
                .await?;
            println!("Replied on Bluesky");
            println!("URI: {}", created.uri);
            if let Some(cid) = created.cid {
                println!("CID: {cid}");
            }
        }
        PostCommand::Like { uri, cid } => {
            let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;
            let created = client.like(&uri, &cid).await?;
            println!("Liked Bluesky post");
            println!("URI: {}", created.uri);
        }
        PostCommand::Unlike { uri } => {
            let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;
            client.unlike(&uri).await?;
            println!("Removed Bluesky like");
        }
        PostCommand::Repost { uri, cid } => {
            let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;
            let created = client.repost(&uri, &cid).await?;
            println!("Reposted Bluesky post");
            println!("URI: {}", created.uri);
        }
        PostCommand::Unrepost { uri } => {
            let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;
            client.unrepost(&uri).await?;
            println!("Removed Bluesky repost");
        }
        PostCommand::List { handle, limit } => {
            let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;
            client.ensure_session().await?;
            let actor = handle.unwrap_or_else(|| client.handle().to_string());
            let feed = client.get_author_feed(&actor, limit).await?;
            output::print_feed(&feed.feed);
        }
    }

    Ok(())
}

async fn handle_bluesky_timeline(command: TimelineCommand, store: &ConfigStore) -> Result<()> {
    match command {
        TimelineCommand::Home { limit } => {
            let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;
            let feed = client.get_timeline(limit).await?;
            output::print_feed(&feed.feed);
        }
    }

    Ok(())
}

async fn handle_bluesky_follow(command: FollowCommand, store: &ConfigStore) -> Result<()> {
    let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;

    match command {
        FollowCommand::Add { handle } => {
            let handle = handle.trim_start_matches('@');
            let profile = client.get_profile(handle).await?;
            let created = client.follow(&profile.did).await?;
            println!("Now following @{handle}");
            println!("URI: {}", created.uri);
        }
        FollowCommand::Remove { handle } => {
            let handle = handle.trim_start_matches('@');
            let profile = client.get_profile(handle).await?;
            client.unfollow(&profile.did).await?;
            println!("Unfollowed @{handle}");
        }
        FollowCommand::List { followers, limit } => {
            client.ensure_session().await?;
            let did = client.did().to_string();
            let users = if followers {
                client.get_followers(&did, limit).await?
            } else {
                client.get_follows(&did, limit).await?
            };
            output::print_profiles(&users);
        }
    }

    Ok(())
}

async fn handle_post(command: cli::TopLevelPostCommand, store: &ConfigStore) -> Result<()> {
    match command {
        cli::TopLevelPostCommand::Create(args) => {
            let encrypt = args.encrypt;
            let e2ee_fallback = args.e2ee_fallback;
            let db = D1Client::new(args.remote)?;
            let draft = PostDraft::from_create_args(cli::CreatePostArgs {
                text: args.text,
                visibility: args.visibility,
                public: args.public,
                protocol: args.protocol,
                encrypt: args.encrypt,
                e2ee_fallback,
                recipients: args.recipients,
                reply_to: args.reply_to,
                object_type: args.object_type,
                title: args.title,
                summary: args.summary,
                starts_at: args.starts_at,
                ends_at: args.ends_at,
                location: args.location,
                poll_options: args.poll_options,
                poll_multiple: args.poll_multiple,
                attachments: args.attachments,
                to: args.to,
                remote: args.remote,
            })?;

            let result = publish_post(draft, store, &db).await?;
            match result {
                PostOutcome::ActivityPub {
                    post_id,
                    read_url,
                    split_key_url,
                    delivery_ids,
                } => {
                    if encrypt {
                        println!("Encrypted ActivityPub post stored");
                        println!("Post: {post_id}");
                        if let Some(read_url) = read_url {
                            println!("Read URL: {read_url}");
                        }
                        match e2ee_fallback {
                            cli::E2eeFallbackMode::Strict => {
                                println!("No decryption key was included in the fallback link.");
                            }
                            cli::E2eeFallbackMode::TrustedServer => {
                                println!(
                                    "Trusted-server fallback selected: the federated fallback link includes the decrypt key fragment."
                                );
                            }
                            cli::E2eeFallbackMode::SplitChannel => {
                                if let Some(split_key_url) = split_key_url {
                                    println!("Split-channel unlock URL: {split_key_url}");
                                }
                                println!(
                                    "The fallback link sent through federation remains keyless."
                                );
                            }
                        }
                        println!("Deliveries queued: {}", delivery_ids.len());
                    } else {
                        println!("Posted to ActivityPub");
                        println!("Post: {post_id}");
                        println!("Deliveries queued: {}", delivery_ids.len());
                    }
                    if !delivery_ids.is_empty() {
                        println!("Delivery IDs:");
                        for delivery_id in delivery_ids {
                            println!("  {delivery_id}");
                        }
                    }
                }
                PostOutcome::Bluesky { uri } => {
                    println!("Posted to Bluesky");
                    println!("URI: {uri}");
                }
                PostOutcome::Both {
                    post_id,
                    uri,
                    delivery_ids,
                } => {
                    println!("Posted to ActivityPub and Bluesky");
                    println!("Post: {post_id}");
                    println!("URI: {uri}");
                    println!("Deliveries queued: {}", delivery_ids.len());
                    if !delivery_ids.is_empty() {
                        println!("Delivery IDs:");
                        for delivery_id in delivery_ids {
                            println!("  {delivery_id}");
                        }
                    }
                }
            }
        }
        cli::TopLevelPostCommand::List(args) => {
            let db = D1Client::new(args.remote)?;
            let posts = db.list_posts(args.limit).await?;
            output::print_posts(&posts);
        }
        cli::TopLevelPostCommand::Update(args) => {
            let db = D1Client::new(args.remote)?;
            let outcome =
                update_activitypub_post(&db, &args.actor, &args.post_id, &args.text).await?;
            print_activity_outcome("Queued ActivityPub Update", &outcome);
        }
        cli::TopLevelPostCommand::Delete(args) => {
            let db = D1Client::new(args.remote)?;
            let outcome = delete_activitypub_post(&db, &args.actor, &args.object_id).await?;
            print_activity_outcome("Queued ActivityPub Delete", &outcome);
        }
        cli::TopLevelPostCommand::Like(args) => {
            let db = D1Client::new(args.remote)?;
            let outcome =
                publish_interaction(&db, &args.actor, &args.object_id, "like", false, args.inbox)
                    .await?;
            print_activity_outcome("Queued ActivityPub Like", &outcome);
        }
        cli::TopLevelPostCommand::Unlike(args) => {
            let db = D1Client::new(args.remote)?;
            let outcome =
                publish_interaction(&db, &args.actor, &args.object_id, "like", true, args.inbox)
                    .await?;
            print_activity_outcome("Queued ActivityPub Undo Like", &outcome);
        }
        cli::TopLevelPostCommand::Boost(args) => {
            let db = D1Client::new(args.remote)?;
            let outcome = publish_interaction(
                &db,
                &args.actor,
                &args.object_id,
                "boost",
                false,
                args.inbox,
            )
            .await?;
            print_activity_outcome("Queued ActivityPub Announce", &outcome);
        }
        cli::TopLevelPostCommand::Unboost(args) => {
            let db = D1Client::new(args.remote)?;
            let outcome =
                publish_interaction(&db, &args.actor, &args.object_id, "boost", true, args.inbox)
                    .await?;
            print_activity_outcome("Queued ActivityPub Undo Announce", &outcome);
        }
    }

    Ok(())
}

fn print_activity_outcome(label: &str, outcome: &ActivityOutcome) {
    println!("{label}");
    println!("Activity: {}", outcome.activity_id);
    println!("Deliveries queued: {}", outcome.delivery_ids.len());
    if !outcome.delivery_ids.is_empty() {
        println!("Delivery IDs:");
        for delivery_id in &outcome.delivery_ids {
            println!("  {delivery_id}");
        }
    }
}

async fn handle_events(command: EventsCommand, store: &ConfigStore) -> Result<()> {
    match command {
        EventsCommand::Create(args) => {
            let db = D1Client::new(args.remote)?;
            let text = args.description.unwrap_or_else(|| args.title.clone());
            let draft = PostDraft {
                text,
                visibility: if args.public {
                    routing::Visibility::Public
                } else {
                    args.visibility
                },
                protocol: Protocol::ActivityPub,
                encrypt: false,
                recipients: BTreeMap::new(),
                reply_to: None,
                to: args.to,
                e2ee_fallback: cli::E2eeFallbackMode::Strict,
                object_type: cli::ActivityObjectType::Event,
                title: Some(args.title),
                summary: None,
                starts_at: Some(args.starts_at),
                ends_at: args.ends_at,
                location: args.location,
                poll_options: Vec::new(),
                poll_multiple: false,
                attachments: Vec::new(),
            };
            match publish_post(draft, store, &db).await? {
                PostOutcome::ActivityPub {
                    post_id,
                    delivery_ids,
                    ..
                } => {
                    println!("Event stored");
                    println!("Post: {post_id}");
                    println!("Deliveries: {}", delivery_ids.len());
                    for id in delivery_ids {
                        println!("  {id}");
                    }
                }
                _ => anyhow::bail!("events publish only through ActivityPub"),
            }
        }
        EventsCommand::Invite(args) => {
            let db = D1Client::new(args.remote)?;
            let now = chrono::Utc::now().to_rfc3339();
            let activity_id = format!("{}#invites/{}", args.event_id, new_local_post_id());
            let activity_json = serde_json::json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "id": activity_id,
                "type": "Invite",
                "actor": args.local_actor,
                "published": now,
                "to": [args.actor],
                "object": args.event_id
            })
            .to_string();
            let delivery_ids = db
                .create_activity_deliveries(d1::ActivityDeliveryInsert {
                    post_id: &args.event_id,
                    actor_id: &args.local_actor,
                    activity_type: "Invite",
                    activity_json: &activity_json,
                    target_inboxes: &[args.inbox],
                })
                .await?;
            println!("Invite queued: {activity_id}");
            println!("Deliveries: {}", delivery_ids.len());
        }
        EventsCommand::Rsvp(args) => {
            let db = D1Client::new(args.remote)?;
            let now = chrono::Utc::now().to_rfc3339();
            let response = args.response.to_string();
            let activity_id = format!(
                "{}#event-{}s/{}",
                args.actor,
                response.to_ascii_lowercase(),
                new_local_post_id()
            );
            let activity_json = serde_json::json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "id": activity_id,
                "type": response,
                "actor": args.actor,
                "published": now,
                "object": args.event_id
            })
            .to_string();
            let target_inboxes: Vec<String> = args.inbox.into_iter().collect();
            let delivery_ids = db
                .create_activity_deliveries(d1::ActivityDeliveryInsert {
                    post_id: &args.event_id,
                    actor_id: &args.actor,
                    activity_type: &response,
                    activity_json: &activity_json,
                    target_inboxes: &target_inboxes,
                })
                .await?;
            println!("{response} queued: {activity_id}");
            println!("Deliveries: {}", delivery_ids.len());
        }
        EventsCommand::List { limit, remote } => {
            let db = D1Client::new(remote)?;
            let events = db.list_events(limit).await?;
            for event in events {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    event.start_time.unwrap_or_else(|| "-".to_string()),
                    event.name.unwrap_or_else(|| "(untitled event)".to_string()),
                    event.visibility.unwrap_or_else(|| "followers".to_string()),
                    event.location.unwrap_or_else(|| "-".to_string()),
                    event.id
                );
            }
        }
    }

    Ok(())
}

async fn handle_media(command: MediaCommand) -> Result<()> {
    match command {
        MediaCommand::Upload(args) => {
            let db = D1Client::new(args.remote)?;
            let filename = args
                .path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| anyhow::anyhow!("media path has no filename"))?;
            let key = args.key.unwrap_or_else(|| {
                format!("{}-{}", chrono::Utc::now().format("%Y%m%d%H%M%S"), filename)
            });
            let url = db.upload_media(&args.bucket, &key, &args.path, &args.public_base_url)?;
            println!("Uploaded media");
            println!("URL: {url}");
            println!("Attachment:");
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "type": "Document",
                    "url": url
                }))?
            );
        }
        MediaCommand::Attachment(args) => {
            let mut value = serde_json::json!({
                "type": args.kind,
                "url": args.url
            });
            if let Some(media_type) = args.media_type {
                value["mediaType"] = serde_json::json!(media_type);
            }
            if let Some(name) = args.name {
                value["name"] = serde_json::json!(name);
            }
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        MediaCommand::EncryptedAttachment(args) => {
            let data = fs::read(&args.path)?;
            let media_type = args
                .media_type
                .unwrap_or_else(|| media_type_for_path(&args.path).to_string());
            let name = args.name.or_else(|| {
                args.path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(ToString::to_string)
            });
            let mut value = serde_json::json!({
                "type": args.kind,
                "mediaType": media_type,
                "data_base64": STANDARD.encode(data)
            });
            if let Some(name) = name {
                value["name"] = serde_json::json!(name);
            }
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
    }

    Ok(())
}

async fn handle_moderation(command: ModerationCommand) -> Result<()> {
    match command {
        ModerationCommand::Blocks { limit, remote } => {
            let db = D1Client::new(remote)?;
            let blocks = db.list_blocks(limit).await?;
            output::print_blocks(&blocks);
        }
        ModerationCommand::BlockActor {
            actor_id,
            reason,
            remote,
        } => {
            let db = D1Client::new(remote)?;
            db.block_actor(&actor_id, reason.as_deref()).await?;
            println!("Blocked actor {actor_id}");
        }
        ModerationCommand::BlockDomain {
            domain,
            reason,
            remote,
        } => {
            let db = D1Client::new(remote)?;
            db.block_domain(&domain, reason.as_deref()).await?;
            println!("Blocked domain {domain}");
        }
        ModerationCommand::Unblock { value, remote } => {
            let db = D1Client::new(remote)?;
            db.unblock(&value).await?;
            println!("Unblocked {value}");
        }
        ModerationCommand::Status { remote } => {
            let db = D1Client::new(remote)?;
            println!("Closed network: {}", db.closed_network_enabled().await?);
            let allowlist = db.list_allowlist_hosts().await?;
            output::print_allowlist(&allowlist);
            let blocks = db.list_blocks(50).await?;
            println!("Blocks: {}", blocks.len());
        }
        ModerationCommand::ClosedNetwork { enabled, remote } => {
            let db = D1Client::new(remote)?;
            db.set_closed_network(enabled).await?;
            println!("Closed network set to {enabled}");
        }
        ModerationCommand::Allow { host, note, remote } => {
            let db = D1Client::new(remote)?;
            db.allow_federation_host(&host, note.as_deref()).await?;
            println!("Allowed federation host {host}");
        }
        ModerationCommand::Disallow { host, remote } => {
            let db = D1Client::new(remote)?;
            db.disallow_federation_host(&host).await?;
            println!("Removed federation allowlist host {host}");
        }
    }

    Ok(())
}

async fn handle_reports(command: ReportsCommand) -> Result<()> {
    match command {
        ReportsCommand::Summary(args) => handle_stats(args).await?,
        ReportsCommand::Activity { limit, remote } => {
            let db = D1Client::new(remote)?;
            let rows = db.activity_report(limit).await?;
            output::print_activity_report(&rows);
        }
        ReportsCommand::TopPosts { limit, remote } => {
            let db = D1Client::new(remote)?;
            let posts = db.top_posts(limit).await?;
            output::print_top_posts(&posts);
        }
    }

    Ok(())
}

async fn handle_sources(command: cli::SourcesCommand) -> Result<()> {
    match command {
        cli::SourcesCommand::Add { command } => match command {
            cli::SourceAddCommand::Rss(args) => {
                let db = D1Client::new(args.remote)?;
                let id = sources::add_source(&db, "rss", args).await?;
                println!("Added RSS source {id}");
            }
            cli::SourceAddCommand::Atom(args) => {
                let db = D1Client::new(args.remote)?;
                let id = sources::add_source(&db, "atom", args).await?;
                println!("Added Atom source {id}");
            }
            cli::SourceAddCommand::Api(args) => {
                let db = D1Client::new(args.remote)?;
                let id = sources::add_source(&db, "api", args).await?;
                println!("Registered API source {id}");
                println!("API refresh adapters are policy/config placeholders in v0.20.");
            }
        },
        cli::SourcesCommand::List { limit, remote } => {
            let db = D1Client::new(remote)?;
            output::print_sources(&db.list_sources(limit).await?);
        }
        cli::SourcesCommand::Remove { id, remote } => {
            let db = D1Client::new(remote)?;
            db.remove_source(&id).await?;
            println!("Removed source {id}");
        }
        cli::SourcesCommand::Refresh {
            id,
            dry_run,
            remote,
        } => {
            let db = D1Client::new(remote)?;
            let sources = if let Some(id) = id {
                vec![db
                    .get_source(&id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("source not found: {id}"))?]
            } else {
                db.active_sources().await?
            };
            if sources.is_empty() {
                println!("No active sources found");
                return Ok(());
            }
            for source in sources {
                match sources::refresh_source(&db, &source, dry_run).await {
                    Ok(report) => {
                        println!(
                            "{} fetched={} parsed={} stored={} {}",
                            report.source_id,
                            report.fetched,
                            report.parsed_items,
                            report.stored_items,
                            report.url
                        );
                    }
                    Err(error) => {
                        let _ = db.mark_source_error(&source.id, &error.to_string()).await;
                        println!("{} error={}", source.id, error);
                    }
                }
            }
        }
        cli::SourcesCommand::Items {
            source_id,
            limit,
            unread,
            remote,
        } => {
            let db = D1Client::new(remote)?;
            let items = db
                .list_source_items(source_id.as_deref(), limit, unread)
                .await?;
            output::print_source_items(&items);
        }
    }

    Ok(())
}

pub(crate) fn new_local_post_id() -> String {
    let mut random = [0u8; 4];
    rand::rngs::OsRng.fill_bytes(&mut random);
    let suffix = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{}-{}", chrono::Utc::now().format("%Y%m%d%H%M%S"), suffix)
}

async fn handle_search(command: SearchCommand) -> Result<()> {
    match command {
        SearchCommand::Posts {
            query,
            remote,
            limit,
        } => {
            let db = D1Client::new(remote)?;
            let posts = db.search_posts(&query, limit).await?;
            output::print_posts(&posts);
        }
        SearchCommand::Users {
            query,
            remote,
            limit,
        } => {
            let db = D1Client::new(remote)?;
            let users = db.search_users(&query, limit).await?;
            output::print_users(&users);
        }
    }

    Ok(())
}

async fn handle_stats(args: cli::StatsArgs) -> Result<()> {
    let db = D1Client::new(args.remote)?;
    let stats = db.stats().await?;
    output::print_server_stats(&stats, args.remote);
    Ok(())
}

async fn handle_e2ee(command: E2eeCommand) -> Result<()> {
    match command {
        E2eeCommand::Encrypt(args) => {
            let mut recipients = BTreeMap::new();
            for recipient in args.recipients {
                let (key_id, path) = recipient.split_once('=').ok_or_else(|| {
                    anyhow::anyhow!("recipient must be in key_id=public_key_pem_file form")
                })?;
                recipients.insert(key_id.to_string(), fs::read_to_string(path)?);
            }

            let payload = e2ee::encrypted_note_payload(
                &args.plaintext,
                &recipients,
                args.view_url.as_deref(),
            )?;
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
        E2eeCommand::Decrypt(args) => {
            let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(args.input)?)?;
            let encrypted = e2ee::encrypted_message_from_json(value)?;
            let private_key = fs::read_to_string(args.private_key)?;
            let plaintext =
                e2ee::decrypt_message(&encrypted, &private_key, args.key_id.as_deref())?;
            println!("{plaintext}");
        }
        E2eeCommand::Fallback { view_url } => {
            println!("{}", e2ee::fallback_content(view_url.as_deref()));
        }
    }

    Ok(())
}

async fn handle_friends(command: FriendsCommand) -> Result<()> {
    match command {
        FriendsCommand::List {
            limit,
            remote,
            actor,
        } => {
            let db = D1Client::new(remote)?;
            let friends = db.list_friends(&actor, limit).await?;
            output::print_friends(&friends);
        }
    }

    Ok(())
}

async fn handle_followers(command: FollowersCommand) -> Result<()> {
    match command {
        FollowersCommand::List { limit, remote } => {
            let db = D1Client::new(remote)?;
            let followers = db.list_followers(limit).await?;
            for follower in followers {
                println!(
                    "{} [{}] {}",
                    follower.follower_actor_id, follower.status, follower.follower_inbox
                );
            }
        }
        FollowersCommand::Approve {
            follower_actor_id,
            remote,
            actor,
            base_url,
        } => {
            let db = D1Client::new(remote)?;
            db.approve_follower(&actor, &follower_actor_id).await?;
            let report =
                delivery::send_follower_accept(&base_url, &actor, &follower_actor_id).await?;
            println!(
                "Approved {} accepted={} inbox={}",
                report.follower_actor_id, report.accepted, report.inbox
            );
        }
        FollowersCommand::Reject {
            follower_actor_id,
            remote,
            actor,
        } => {
            let db = D1Client::new(remote)?;
            db.reject_follower(&actor, &follower_actor_id).await?;
            println!("Rejected {follower_actor_id}");
        }
    }

    Ok(())
}

async fn handle_owner(command: OwnerCommand, store: &ConfigStore) -> Result<()> {
    match command {
        OwnerCommand::Snapshot(args) => {
            let snapshot = owner_api(&args)
                .snapshot()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_snapshot(&snapshot);
        }
        OwnerCommand::Profile(command) => handle_owner_profile(command).await?,
        OwnerCommand::Timeline(args) => {
            let posts = owner_api(&args.api)
                .home_timeline(args.limit, args.include_replies)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            for post in &posts {
                let author = post
                    .actor_display_name
                    .as_deref()
                    .or(post.actor_username.as_deref())
                    .unwrap_or(&post.actor_id);
                println!(
                    "{} [{}] {}",
                    author,
                    post.visibility,
                    post.published_at.as_deref().unwrap_or("")
                );
                println!("{}", post.content);
                println!();
            }
            if posts.is_empty() {
                println!("No followed posts found");
            }
        }
        OwnerCommand::Followers(args) => {
            let followers = owner_api(&args.api)
                .followers(args.limit)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_followers(&followers);
        }
        OwnerCommand::Following(args) => {
            let following = owner_api(&args.api)
                .following(args.limit)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_following(&following);
        }
        OwnerCommand::Friends(args) => {
            let friends = owner_api(&args)
                .friends()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_friends(&friends);
        }
        OwnerCommand::Notifications(args) => {
            let notifications = owner_api(&args)
                .notifications()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_notifications(&notifications);
        }
        OwnerCommand::NotificationRead(args) => {
            owner_api(&args.api)
                .mark_notification_read(&args.id)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("Marked notification {} read", args.id);
        }
        OwnerCommand::Deliveries(args) => {
            let deliveries = owner_api(&args)
                .deliveries()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_deliveries(&deliveries);
        }
        OwnerCommand::Dms(args) => {
            let messages = owner_api(&args)
                .direct_messages()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_direct_messages(&messages);
        }
        OwnerCommand::E2eeMessages(args) => {
            let messages = owner_api(&args)
                .e2ee_messages()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_e2ee_messages(&messages);
        }
        OwnerCommand::E2eeSend(args) => {
            send_owner_e2ee_message(args).await?;
        }
        OwnerCommand::E2eeGroupSend(args) => {
            send_owner_e2ee_group_message(args).await?;
        }
        OwnerCommand::E2eeDecrypt(args) => {
            decrypt_owner_e2ee_message(args, store).await?;
        }
        OwnerCommand::E2eeDevices(args) => {
            let devices = owner_api(&args)
                .e2ee_devices()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_e2ee_devices(&devices);
        }
        OwnerCommand::E2eeDeviceInit(args) => {
            init_owner_e2ee_device(args, store).await?;
        }
        OwnerCommand::E2eeDeviceRevoke(args) => {
            let device = owner_api(&args.api)
                .revoke_e2ee_device(&OwnerE2eeDeviceRef {
                    device_id: args.device_id,
                })
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_e2ee_devices(&[device]);
        }
        OwnerCommand::E2eeDeviceRotate(args) => {
            rotate_owner_e2ee_device(args, store).await?;
        }
        OwnerCommand::E2eeKeys(args) => {
            print_owner_e2ee_keys(store, args.instance_url.as_deref())?;
        }
        OwnerCommand::E2eeRecovery(args) => {
            print_owner_e2ee_recovery(&args, store).await?;
        }
        OwnerCommand::E2eeKeyExport(args) => {
            export_owner_e2ee_key(store, args)?;
        }
        OwnerCommand::E2eePeers(args) => {
            let devices = owner_api(&args)
                .e2ee_peer_devices()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_e2ee_peer_devices(&devices);
        }
        OwnerCommand::E2eePeerDiscover(args) => {
            let result = owner_api(&args.api)
                .discover_e2ee_peer_devices(&OwnerE2eePeerDiscoverRequest {
                    actor_id: args.actor_id,
                })
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("actor={}", result.actor_id);
            print_owner_e2ee_peer_devices(&result.items);
        }
        OwnerCommand::E2eePeerTrust(args) => {
            let peer = trust_owner_e2ee_peer(args).await?;
            print_owner_e2ee_peer_devices(&[peer]);
        }
        OwnerCommand::E2eePeerRevoke(args) => {
            let peer = owner_api(&args.api)
                .revoke_e2ee_peer_device(&OwnerE2eePeerDeviceRef {
                    actor_id: args.actor_id,
                    device_id: args.device_id,
                })
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_e2ee_peer_devices(&[peer]);
        }
        OwnerCommand::Search(args) => {
            let results = owner_api(&args.api)
                .search_with_options(&OwnerSearchQuery {
                    query: args.query.clone(),
                    scope: args.scope.clone(),
                    confirm_public_sensitive: args.confirm_public_sensitive,
                    provider: args.provider.clone(),
                    result_type: args.result_type.clone(),
                    servers: args.servers.clone(),
                    sort: args.sort.clone(),
                    since: args.since.clone(),
                    until: args.until.clone(),
                    author: args.author.clone(),
                    mentions: args.mentions.clone(),
                    lang: args.lang.clone(),
                    domain: args.domain.clone(),
                    url: args.url.clone(),
                    tags: args.tags.clone(),
                })
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_search(&results);
        }
        OwnerCommand::Stats(args) => {
            let stats = owner_api(&args)
                .stats()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_stats(&stats);
        }
        OwnerCommand::Diagnostics(args) => {
            let diagnostics = owner_api(&args)
                .diagnostics()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_diagnostics(&diagnostics);
        }
        OwnerCommand::PostCreate(args) => {
            let visibility = if args.public {
                Visibility::Public
            } else {
                args.visibility
            };
            let created = owner_api(&args.api)
                .create_post(&OwnerComposeDraft {
                    text: args.text,
                    visibility: owner_visibility(visibility),
                    protocol: owner_protocol(args.protocol),
                    encrypt: args.encrypt,
                    in_reply_to: args.reply_to,
                    audience_list_id: None,
                    recipients: args.recipients,
                    attachments: args.attachments,
                })
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("Created owner API post");
            println!("id={}", created.id);
            println!("visibility={}", created.visibility);
            println!("audience={}", audience_description(&created.visibility));
            println!("protocol={}", created.protocol);
            println!("published_at={}", created.published_at);
            if let Some(reply) = created.in_reply_to {
                println!("in_reply_to={reply}");
            }
            if !created.delivery_ids.is_empty() {
                println!("delivery_ids={}", created.delivery_ids.join(","));
            }
        }
        OwnerCommand::PostDelete(args) => {
            let deleted = owner_api(&args.api)
                .delete_post(&args.object_id)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("Deleted owner API post");
            println!("id={}", deleted.id);
            println!("deleted={}", deleted.deleted);
            if !deleted.delivery_ids.is_empty() {
                println!("delivery_ids={}", deleted.delivery_ids.join(","));
            }
        }
        OwnerCommand::Sources(args) => {
            let sources = owner_api(&args)
                .sources()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_sources(&sources);
        }
        OwnerCommand::Watches(args) => {
            let watches = owner_api(&args)
                .watches()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_sources(&watches);
        }
        OwnerCommand::MediaUpload(args) => {
            let data = fs::read(&args.path)?;
            let filename = args
                .filename
                .or_else(|| {
                    args.path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(ToString::to_string)
                })
                .ok_or_else(|| anyhow::anyhow!("media path has no filename"))?;
            let media_type = args
                .media_type
                .or_else(|| Some(media_type_for_path(&args.path).to_string()));
            let uploaded = owner_api(&args.api)
                .upload_media(&OwnerMediaUpload {
                    filename,
                    media_type,
                    description: args.description,
                    access: args.access,
                    expires_in_seconds: args.expires_in_seconds,
                    require_authorized_fetch: args.require_authorized_fetch.then_some(true),
                    data_base64: STANDARD.encode(data),
                })
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("Uploaded owner API media");
            println!("url={}", uploaded.url);
            if let Some(media_type) = uploaded.media_type {
                println!("media_type={media_type}");
            }
            if let Some(access) = uploaded.access {
                println!("access={access}");
            }
            if let Some(authorized_fetch) = uploaded.authorized_fetch {
                println!("authorized_fetch={authorized_fetch}");
            }
            if let Some(expires_at) = uploaded.expires_at {
                println!("expires_at={expires_at}");
            }
            println!(
                "attachment={}",
                serde_json::to_string(&uploaded.attachment)?
            );
        }
        OwnerCommand::MediaRevoke(args) => {
            owner_api(&args.api)
                .revoke_media(&args.url)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("Revoked owner API media");
            println!("url={}", args.url);
        }
        OwnerCommand::SourceAdd(args) => {
            let result = owner_api(&args.api)
                .add_source(&OwnerSourceAdd {
                    source_type: args.source_type,
                    url: args.url,
                    title: args.title,
                    cadence_minutes: args.cadence_minutes,
                    api_secret_name: args.api_secret_name,
                    private_reader_only: args.private_reader_only,
                    excerpt_only: args.excerpt_only,
                    link_required: args.link_required,
                    attribution_required: args.attribution_required,
                    image_allowed: args.image_allowed,
                    full_text_allowed: args.full_text_allowed,
                })
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!(
                "Added source {} {} {}",
                result.source.id, result.source.source_type, result.source.url
            );
        }
        OwnerCommand::SourceRemove(args) => {
            owner_api(&args.api)
                .remove_source(&args.id)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("Removed source {}", args.id);
        }
        OwnerCommand::SourceRefresh(args) => {
            let result = owner_api(&args.api)
                .refresh_sources(args.id.as_deref())
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            for item in result.items {
                if item.ok {
                    println!(
                        "{} ok status={}",
                        item.id,
                        item.status.unwrap_or_else(|| "active".to_string())
                    );
                } else {
                    println!("{} error={}", item.id, item.error.unwrap_or_default());
                }
            }
        }
        OwnerCommand::WatchAdd(args) => {
            let result = owner_api(&args.api)
                .add_watch(&OwnerWatchAdd {
                    watch_type: args.watch_type,
                    target: args.target,
                    title: args.title,
                    cadence_minutes: args.cadence_minutes,
                    private_reader_only: args.private_reader_only,
                    excerpt_only: args.excerpt_only,
                    link_required: args.link_required,
                    attribution_required: args.attribution_required,
                    image_allowed: args.image_allowed,
                    full_text_allowed: args.full_text_allowed,
                })
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!(
                "Added watch {} {} {}",
                result.source.id, result.source.source_type, result.source.url
            );
        }
        OwnerCommand::WatchRemove(args) => {
            owner_api(&args.api)
                .remove_watch(&args.id)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("Removed watch {}", args.id);
        }
        OwnerCommand::WatchRefresh(args) => {
            let result = owner_api(&args.api)
                .refresh_watches(args.id.as_deref())
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            for item in result.items {
                if item.ok {
                    println!(
                        "{} ok status={}",
                        item.id,
                        item.status.unwrap_or_else(|| "active".to_string())
                    );
                } else {
                    println!("{} error={}", item.id, item.error.unwrap_or_default());
                }
            }
        }
        OwnerCommand::Moderation(args) => {
            let moderation = owner_api(&args)
                .moderation()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_moderation(&moderation);
        }
        OwnerCommand::BlockActor(args) => {
            owner_api(&args.api)
                .block_actor(&args.actor_id, args.reason.as_deref())
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("Blocked actor {}", args.actor_id);
        }
        OwnerCommand::BlockDomain(args) => {
            owner_api(&args.api)
                .block_domain(&args.domain, args.reason.as_deref())
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("Blocked domain {}", args.domain);
        }
        OwnerCommand::Unblock(args) => {
            owner_api(&args.api)
                .unblock(&args.value)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("Unblocked {}", args.value);
        }
        OwnerCommand::AllowHost(args) => {
            owner_api(&args.api)
                .allow_host(&args.host, args.note.as_deref())
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("Allowed host {}", args.host);
        }
        OwnerCommand::DisallowHost(args) => {
            owner_api(&args.api)
                .disallow_host(&args.host)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("Removed allowlist host {}", args.host);
        }
        OwnerCommand::Discover(args) => {
            let actor = owner_api(&args.api)
                .discover_actor(&args.target)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_discovered_actor(&actor);
        }
        OwnerCommand::Post(args) => {
            let detail = owner_api(&args.api)
                .post_detail(&args.object_id)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_post_detail(&detail);
        }
        OwnerCommand::Link(args) => {
            let detail = owner_api(&args.api)
                .post_detail(&args.object_id)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!("{}", detail.post.id);
        }
        OwnerCommand::Open(args) => {
            let detail = owner_api(&args.api)
                .post_detail(&args.object_id)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            open_url(&detail.post.id)?;
            println!("Opened {}", detail.post.id);
        }
        OwnerCommand::Follow(args) => {
            let result = owner_api(&args.api)
                .follow_actor(&args.target)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!(
                "Follow requested: {} [{}]",
                result.following.target_actor_id, result.following.status
            );
            if !result.delivery_ids.is_empty() {
                println!("deliveries={}", result.delivery_ids.join(","));
            }
        }
        OwnerCommand::Unfollow(args) => {
            let result = owner_api(&args.api)
                .unfollow_actor(&args.target)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            println!(
                "Unfollow requested: {} [{}]",
                result.following.target_actor_id, result.following.status
            );
            if !result.delivery_ids.is_empty() {
                println!("deliveries={}", result.delivery_ids.join(","));
            }
        }
        OwnerCommand::Like(args) => owner_interact(&args.api, &args.object_id, "like").await?,
        OwnerCommand::Unlike(args) => owner_interact(&args.api, &args.object_id, "unlike").await?,
        OwnerCommand::Boost(args) => owner_interact(&args.api, &args.object_id, "boost").await?,
        OwnerCommand::Unboost(args) => {
            owner_interact(&args.api, &args.object_id, "unboost").await?
        }
    }

    Ok(())
}

fn open_url(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(url).status()?;

    #[cfg(target_os = "linux")]
    let status = std::process::Command::new("xdg-open").arg(url).status()?;

    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .status()?;

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    return Err(anyhow::anyhow!(
        "opening URLs is not supported on this platform"
    ));

    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("failed to open {url}"))
    }
}

async fn handle_owner_profile(command: cli::OwnerProfileCommand) -> Result<()> {
    match command {
        cli::OwnerProfileCommand::Show(args) => {
            let snapshot = owner_api(&args)
                .snapshot()
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_profile(&snapshot.profile);
        }
        cli::OwnerProfileCommand::Update(args) => {
            let profile = owner_api(&args.api)
                .update_profile(&OwnerProfileUpdate {
                    actor_type: args.actor_type,
                    display_name: args.display_name,
                    summary: args.summary,
                    icon: args.icon,
                    image: args.image,
                })
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            print_owner_profile(&profile);
        }
    }
    Ok(())
}

async fn init_owner_e2ee_device(
    args: cli::OwnerE2eeDeviceInitArgs,
    store: &ConfigStore,
) -> Result<()> {
    let mut rng = OsRng;
    let private_key = RsaPrivateKey::new(&mut rng, 2048)?;
    let public_key = RsaPublicKey::from(&private_key);
    let private_pem = private_key.to_pkcs8_pem(LineEnding::LF)?;
    let public_pem = public_key.to_public_key_pem(LineEnding::LF)?;

    let private_key_path = if let Some(path) = args.private_key_out.as_ref() {
        if path.exists() && !args.force {
            return Err(anyhow::anyhow!(
                "{} already exists; pass --force to overwrite",
                path.display()
            ));
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(path, private_pem.as_bytes())?;
        set_private_key_permissions(path)?;
        path.clone()
    } else {
        store.save_e2ee_private_key(
            &args.api.instance_url,
            &args.device_id,
            private_pem.as_ref(),
            args.force,
        )?
    };

    let device = owner_api(&args.api)
        .upsert_e2ee_device(&OwnerE2eeDeviceUpsert {
            device_id: args.device_id.clone(),
            display_name: args.display_name,
            protocol: "dais-mls-v1".to_string(),
            credential: public_pem.clone(),
            key_package: public_pem,
        })
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    println!("Published E2EE device");
    println!("device_id={}", device.device_id);
    println!("actor={}", device.actor_id);
    println!("fingerprint={}", device.fingerprint);
    println!("private_key={}", private_key_path.display());
    println!("protocol={}", device.protocol);
    println!("status={}", device.status);
    println!("wire_material=v1-rsa-fallback");
    println!(
        "Back up this private key; losing it means old encrypted messages cannot be decrypted."
    );
    Ok(())
}

async fn rotate_owner_e2ee_device(
    args: cli::OwnerE2eeDeviceRotateArgs,
    store: &ConfigStore,
) -> Result<()> {
    if args.old_device_id == args.new_device_id {
        return Err(anyhow::anyhow!(
            "--new-device-id must differ from --old-device-id"
        ));
    }
    println!("Revoking old E2EE device");
    let revoked = owner_api(&args.api)
        .revoke_e2ee_device(&OwnerE2eeDeviceRef {
            device_id: args.old_device_id.clone(),
        })
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    print_owner_e2ee_devices(&[revoked]);
    println!(
        "Rotation note: keep the old private key backup if you need to decrypt messages addressed to {}. Revocation only stops future publication/use.",
        args.old_device_id
    );

    init_owner_e2ee_device(
        cli::OwnerE2eeDeviceInitArgs {
            api: args.api,
            device_id: args.new_device_id,
            display_name: args.display_name,
            private_key_out: args.private_key_out,
            force: args.force,
        },
        store,
    )
    .await
}

async fn send_owner_e2ee_message(args: cli::OwnerE2eeSendArgs) -> Result<()> {
    let recipient_public_key = resolve_owner_e2ee_recipient_key(&args).await?;
    let mut recipients = BTreeMap::new();
    recipients.insert(args.recipient_device_id.clone(), recipient_public_key);
    let payload =
        e2ee::encrypted_note_payload(&args.plaintext, &recipients, args.view_url.as_deref())?;
    let encrypted_message = serde_json::to_value(&payload.encrypted_message)?;

    let result = owner_api(&args.api)
        .send_e2ee_message(&OwnerE2eeMessageSend {
            recipient_actor_id: args.recipient_actor_id,
            recipient_device_id: Some(args.recipient_device_id),
            sender_device_id: args.sender_device_id,
            dais_encrypted_message: None,
            encrypted_message: Some(encrypted_message),
            fallback_content: Some(payload.content),
        })
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    println!("Sent owner E2EE message");
    println!("id={}", result.message.id);
    println!("conversation_id={}", result.message.conversation_id);
    println!("recipient_actor={:?}", result.message.recipient_actor_id);
    println!("delivery_ids={}", result.delivery_ids.join(","));
    Ok(())
}

async fn send_owner_e2ee_group_message(args: cli::OwnerE2eeGroupSendArgs) -> Result<()> {
    let api = owner_api(&args.api);
    let audience_lists = api
        .audience_lists()
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let audience = audience_lists
        .iter()
        .find(|list| list.id == args.audience_list_id)
        .ok_or_else(|| anyhow::anyhow!("audience group {} not found", args.audience_list_id))?;
    let peers = api
        .e2ee_peer_devices()
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let plan = plan_owner_e2ee_group_recipients(audience, &peers, args.allow_untrusted)?;
    let payload =
        e2ee::encrypted_note_payload(&args.plaintext, &plan.recipients, args.view_url.as_deref())?;
    let encrypted_message = serde_json::to_value(&payload.encrypted_message)?;

    println!("Sending owner E2EE group message");
    println!("audience_id={}", audience.id);
    println!("audience_name={}", audience.name);
    println!("member_count={}", plan.member_count);
    println!("recipient_device_count={}", plan.recipients.len());
    println!("wire_material=v1-rsa-fallback-group");
    println!(
        "Recovery note: every recipient needs a matching private key; lost keys cannot decrypt this message later."
    );

    let mut all_delivery_ids = Vec::new();
    for delivery in &plan.deliveries {
        let result = api
            .send_e2ee_message(&OwnerE2eeMessageSend {
                recipient_actor_id: delivery.actor_id.clone(),
                recipient_device_id: Some(delivery.validation_device_id.clone()),
                sender_device_id: args.sender_device_id.clone(),
                dais_encrypted_message: None,
                encrypted_message: Some(encrypted_message.clone()),
                fallback_content: Some(payload.content.clone()),
            })
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        all_delivery_ids.extend(result.delivery_ids);
        println!(
            "sent actor={} validation_device={} message={}",
            delivery.actor_id, delivery.validation_device_id, result.message.id
        );
    }

    println!("delivery_ids={}", all_delivery_ids.join(","));
    Ok(())
}

async fn decrypt_owner_e2ee_message(
    args: cli::OwnerE2eeDecryptArgs,
    store: &ConfigStore,
) -> Result<()> {
    let messages = owner_api(&args.api)
        .e2ee_messages()
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let message = messages
        .into_iter()
        .find(|message| message.id == args.message_id)
        .ok_or_else(|| anyhow::anyhow!("owner E2EE message {} not found", args.message_id))?;
    if message.e2ee_protocol == "mls-rfc9420" {
        return Err(anyhow::anyhow!(
            "message {} is MLS E2EE group={} epoch={}; v1 RSA fallback decrypt cannot decrypt MLS state",
            message.id,
            message.mls_group_id.as_deref().unwrap_or("unknown"),
            message
                .mls_epoch
                .map(|epoch| epoch.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ));
    }
    let encrypted = e2ee::encrypted_message_from_json(message.encrypted_message)?;
    let private_key = if let Some(path) = args.private_key.as_ref() {
        fs::read_to_string(path)?
    } else {
        let device_id = args.device_id.as_deref().ok_or_else(|| {
            anyhow::anyhow!("--device-id is required when --private-key is omitted")
        })?;
        store.load_e2ee_private_key(&args.api.instance_url, device_id)?
    };
    let plaintext = e2ee::decrypt_message(&encrypted, &private_key, args.key_id.as_deref())?;
    println!("{plaintext}");
    Ok(())
}

fn print_owner_e2ee_keys(store: &ConfigStore, instance_url: Option<&str>) -> Result<()> {
    let entries = store.list_e2ee_private_keys()?;
    let instance_filter = instance_url.map(|value| {
        store
            .e2ee_private_key_path(value, "placeholder")
            .parent()
            .and_then(|path| path.file_name())
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default()
    });
    let mut printed = 0usize;
    for entry in entries {
        if instance_filter
            .as_ref()
            .is_some_and(|filter| filter != &entry.instance)
        {
            continue;
        }
        println!("{} {}", entry.instance, entry.device_id);
        println!("path={}", entry.path.display());
        printed += 1;
    }
    if printed == 0 {
        println!("No stored E2EE private keys found");
    }
    Ok(())
}

fn export_owner_e2ee_key(store: &ConfigStore, args: cli::OwnerE2eeKeyExportArgs) -> Result<()> {
    if args.output.exists() && !args.force {
        return Err(anyhow::anyhow!(
            "{} already exists; pass --force to overwrite",
            args.output.display()
        ));
    }
    let private_key = store.load_e2ee_private_key(&args.instance_url, &args.device_id)?;
    if let Some(parent) = args.output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&args.output, private_key)?;
    set_private_key_permissions(&args.output)?;
    println!("Exported E2EE private key");
    println!("device_id={}", args.device_id);
    println!("output={}", args.output.display());
    println!("Store this backup securely; anyone with it can decrypt messages for this device.");
    Ok(())
}

async fn print_owner_e2ee_recovery(args: &cli::OwnerApiArgs, store: &ConfigStore) -> Result<()> {
    let devices = owner_api(args)
        .e2ee_devices()
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let local_key_ids = local_e2ee_key_device_ids(store, &args.instance_url)?;
    let mut missing_active = 0usize;
    let mut revoked_with_key = 0usize;

    println!("E2EE recovery status");
    println!("instance={}", args.instance_url);
    for device in &devices {
        let has_local_key = local_key_ids.contains(&device.device_id);
        if device.status == "active" && !has_local_key {
            missing_active += 1;
        }
        if device.status == "revoked" && has_local_key {
            revoked_with_key += 1;
        }
        println!(
            "{} [{}] fingerprint={} local_private_key={}",
            device.device_id,
            device.status,
            device.fingerprint,
            if has_local_key { "present" } else { "missing" }
        );
    }

    if missing_active == 0 {
        println!("active_key_status=ok");
    } else {
        println!("active_key_status=missing_private_key");
        println!(
            "WARNING: active devices without local private keys can receive encrypted messages that this client cannot decrypt."
        );
    }
    if revoked_with_key > 0 {
        println!(
            "revoked_key_note=keep revoked-device private keys if old encrypted messages still need to be decrypted."
        );
    }
    println!(
        "Recovery boundary: Dais cannot recover old encrypted content after all recipient private keys are lost."
    );
    Ok(())
}

fn local_e2ee_key_device_ids(
    store: &ConfigStore,
    instance_url: &str,
) -> Result<std::collections::BTreeSet<String>> {
    let instance_dir = store
        .e2ee_private_key_path(instance_url, "placeholder")
        .parent()
        .and_then(|path| path.file_name())
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    let ids = store
        .list_e2ee_private_keys()?
        .into_iter()
        .filter(|entry| entry.instance == instance_dir)
        .map(|entry| entry.device_id)
        .collect();
    Ok(ids)
}

async fn trust_owner_e2ee_peer(args: cli::OwnerE2eePeerRefArgs) -> Result<OwnerE2eePeerDevice> {
    let peers = owner_api(&args.api)
        .e2ee_peer_devices()
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let peer = peers
        .into_iter()
        .find(|peer| peer.actor_id == args.actor_id && peer.device_id == args.device_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "E2EE peer device {} for {} not found; run e2ee-peer-discover first",
                args.device_id,
                args.actor_id
            )
        })?;
    owner_api(&args.api)
        .trust_e2ee_peer_device(&OwnerE2eePeerTrustRequest {
            actor_id: peer.actor_id,
            device_id: peer.device_id,
            display_name: peer.display_name,
            protocol: peer.protocol,
            credential: peer.credential,
            key_package: peer.key_package,
            fingerprint: Some(peer.fingerprint),
        })
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

async fn resolve_owner_e2ee_recipient_key(args: &cli::OwnerE2eeSendArgs) -> Result<String> {
    if let Some(path) = args.recipient_public_key.as_ref() {
        return Ok(fs::read_to_string(path)?);
    }

    let api = owner_api(&args.api);
    let local_devices = api
        .e2ee_devices()
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    if let Some(device) = local_devices.iter().find(|device| {
        device.actor_id == args.recipient_actor_id && device.device_id == args.recipient_device_id
    }) {
        return Ok(device.credential.clone());
    }

    let peers = api
        .e2ee_peer_devices()
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let peer = peers
        .iter()
        .find(|peer| {
            peer.actor_id == args.recipient_actor_id
                && peer.device_id == args.recipient_device_id
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "recipient E2EE device {} for {} not found; provide --recipient-public-key or run e2ee-peer-discover",
                args.recipient_device_id,
                args.recipient_actor_id
            )
        })?;
    if peer.trust_state != "trusted" && !args.allow_untrusted {
        return Err(anyhow::anyhow!(
            "recipient E2EE device {} for {} is {}; run e2ee-peer-trust or pass --allow-untrusted",
            peer.device_id,
            peer.actor_id,
            peer.trust_state
        ));
    }
    Ok(peer.credential.clone())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OwnerE2eeGroupPlan {
    member_count: usize,
    recipients: BTreeMap<String, String>,
    deliveries: Vec<OwnerE2eeGroupDelivery>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OwnerE2eeGroupDelivery {
    actor_id: String,
    validation_device_id: String,
}

fn plan_owner_e2ee_group_recipients(
    audience: &OwnerAudienceList,
    peers: &[OwnerE2eePeerDevice],
    allow_untrusted: bool,
) -> Result<OwnerE2eeGroupPlan> {
    let members: Vec<String> = audience
        .member_actor_ids
        .iter()
        .map(|member| member.trim())
        .filter(|member| !member.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if members.is_empty() {
        return Err(anyhow::anyhow!(
            "audience group {} has no members",
            audience.id
        ));
    }

    let mut recipients = BTreeMap::new();
    let mut deliveries = Vec::new();
    for actor_id in &members {
        let devices: Vec<&OwnerE2eePeerDevice> = peers
            .iter()
            .filter(|peer| {
                peer.actor_id == *actor_id
                    && (peer.trust_state == "trusted" || allow_untrusted)
                    && peer.revoked_at.is_none()
            })
            .collect();
        if devices.is_empty() {
            return Err(anyhow::anyhow!(
                "audience member {} has no {}E2EE peer devices; run e2ee-peer-discover and e2ee-peer-trust first",
                actor_id,
                if allow_untrusted { "discovered " } else { "trusted " }
            ));
        }

        for device in &devices {
            let key_id = owner_e2ee_group_key_id(&device.actor_id, &device.device_id);
            recipients.insert(key_id, device.credential.clone());
        }
        deliveries.push(OwnerE2eeGroupDelivery {
            actor_id: actor_id.clone(),
            validation_device_id: devices[0].device_id.clone(),
        });
    }

    Ok(OwnerE2eeGroupPlan {
        member_count: members.len(),
        recipients,
        deliveries,
    })
}

fn owner_e2ee_group_key_id(actor_id: &str, device_id: &str) -> String {
    format!("{}#{}", actor_id.trim_end_matches('#'), device_id)
}

#[cfg(unix)]
fn set_private_key_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_key_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

async fn owner_interact(
    args: &cli::OwnerApiArgs,
    object_id: &str,
    interaction: &str,
) -> Result<()> {
    let result = owner_api(args)
        .interact(&OwnerInteraction {
            object_id: object_id.to_string(),
            interaction: interaction.to_string(),
        })
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    println!(
        "{} {} deliveries={}",
        result.interaction,
        result.object_id,
        result.delivery_ids.len()
    );
    if !result.delivery_ids.is_empty() {
        println!("delivery_ids={}", result.delivery_ids.join(","));
    }
    Ok(())
}

fn owner_api(args: &cli::OwnerApiArgs) -> OwnerApiClient {
    OwnerApiClient::new(&args.instance_url, &args.owner_token)
}

fn owner_visibility(value: Visibility) -> OwnerVisibility {
    match value {
        Visibility::Public => OwnerVisibility::Public,
        Visibility::Unlisted => OwnerVisibility::Unlisted,
        Visibility::Followers => OwnerVisibility::Followers,
        Visibility::Direct => OwnerVisibility::Direct,
    }
}

fn owner_protocol(value: Protocol) -> OwnerProtocolRoute {
    match value {
        Protocol::ActivityPub => OwnerProtocolRoute::ActivityPub,
        Protocol::Atproto => OwnerProtocolRoute::AtProto,
        Protocol::Both => OwnerProtocolRoute::Both,
    }
}

fn media_type_for_path(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

fn print_owner_snapshot(snapshot: &OwnerSnapshot) {
    println!("profile={}", snapshot.profile.public_handle);
    println!("posts={}", snapshot.posts.len());
    println!("timeline={}", snapshot.home_timeline.len());
    println!("followers={}", snapshot.followers.len());
    println!("friends={}", snapshot.friends.len());
    println!("following={}", snapshot.following.len());
    println!("sources={}", snapshot.sources.len());
    for diagnostic in &snapshot.diagnostics {
        println!(
            "diagnostic {}={} {}",
            diagnostic.key,
            if diagnostic.ok { "ok" } else { "warn" },
            diagnostic.detail
        );
    }
}

fn print_owner_profile(profile: &OwnerProfile) {
    println!("public_handle={}", profile.public_handle);
    println!("actor_url={}", profile.actor_url);
    println!("username={}", profile.username);
    println!("actor_type={}", profile.actor_type);
    println!(
        "display_name={}",
        profile.display_name.as_deref().unwrap_or("")
    );
    println!("summary={}", profile.summary.as_deref().unwrap_or(""));
    println!(
        "icon={}",
        profile
            .icon
            .as_deref()
            .or(profile.avatar_url.as_deref())
            .unwrap_or("")
    );
    println!(
        "image={}",
        profile
            .image
            .as_deref()
            .or(profile.header_url.as_deref())
            .unwrap_or("")
    );
    println!("public_surfaces=ActivityPub actor JSON, HTML profile, Mastodon account API");
}

fn print_owner_friends(friends: &[OwnerFriend]) {
    if friends.is_empty() {
        println!("No friends found");
        return;
    }
    for row in friends {
        println!(
            "{} inbox={} shared_inbox={} follower_since={} following_since={} accepted_at={}",
            row.friend_actor_id,
            row.friend_inbox.as_deref().unwrap_or(""),
            row.friend_shared_inbox.as_deref().unwrap_or(""),
            row.follower_since.as_deref().unwrap_or(""),
            row.following_since.as_deref().unwrap_or(""),
            row.accepted_at.as_deref().unwrap_or("")
        );
    }
}

fn print_owner_followers(followers: &[OwnerFollower]) {
    println!("graph_visibility=operator-only");
    println!("graph_note=Followers are not advertised publicly by Dais by default.");
    if followers.is_empty() {
        println!("No followers found");
        return;
    }
    for row in followers {
        println!(
            "{} [{}] inbox={} shared_inbox={} updated_at={}",
            row.follower_actor_id,
            row.status,
            row.follower_inbox,
            row.follower_shared_inbox.as_deref().unwrap_or(""),
            row.updated_at.as_deref().unwrap_or("")
        );
    }
}

fn print_owner_following(following: &[OwnerFollowing]) {
    println!("graph_visibility=operator-only");
    println!("graph_note=Following is private by default; audit this list for sensitive follows.");
    if following.is_empty() {
        println!("No following actors found");
        return;
    }
    for row in following {
        println!(
            "{} [{}] inbox={} accepted_at={}",
            row.target_actor_id,
            row.status,
            row.target_inbox,
            row.accepted_at.as_deref().unwrap_or("")
        );
    }
}

fn print_owner_notifications(notifications: &[OwnerNotification]) {
    if notifications.is_empty() {
        println!("No notifications found");
        return;
    }
    for notification in notifications {
        let actor = notification
            .actor_display_name
            .as_deref()
            .or(notification.actor_username.as_deref())
            .unwrap_or(&notification.actor_id);
        println!(
            "{} [{}] {} {} {}",
            notification.id,
            notification.kind,
            actor,
            if owner_notification_read(notification) {
                "read"
            } else {
                "unread"
            },
            notification.created_at.as_deref().unwrap_or("")
        );
        if let Some(post_id) = notification.post_id.as_deref() {
            println!("post={post_id}");
        }
        if let Some(content) = notification.content.as_deref() {
            println!("{content}");
        }
        if let Some(context) = notification.context_post_content.as_deref() {
            println!("context={context}");
        }
        println!();
    }
}

fn owner_notification_read(notification: &OwnerNotification) -> bool {
    notification.read == serde_json::Value::Bool(true)
        || notification.read == serde_json::Value::Number(1.into())
        || notification.read == serde_json::Value::String("1".to_string())
        || notification.read == serde_json::Value::String("true".to_string())
}

fn print_owner_deliveries(deliveries: &[OwnerDelivery]) {
    if deliveries.is_empty() {
        println!("No deliveries found");
        return;
    }
    for delivery in deliveries {
        println!(
            "{} [{}] {} retries={}",
            delivery.id,
            delivery.status,
            delivery.protocol,
            delivery.retry_count.unwrap_or(0)
        );
        println!("post={}", delivery.post_id);
        println!("target={}", delivery.target_url);
        if let Some(activity_type) = delivery.activity_type.as_deref() {
            println!("activity={activity_type}");
        }
        if let Some(error) = delivery.error_message.as_deref() {
            println!("error={error}");
        }
        println!();
    }
}

fn print_owner_direct_messages(messages: &[OwnerDirectMessage]) {
    if messages.is_empty() {
        println!("No direct messages found");
        return;
    }
    for message in messages {
        println!(
            "{} [{}] {}",
            message.id, message.conversation_id, message.published_at
        );
        println!("sender={}", message.sender_id);
        println!("{}", message.content);
        println!();
    }
}

fn print_owner_e2ee_messages(messages: &[OwnerE2eeMessage]) {
    if messages.is_empty() {
        println!("No owner E2EE messages found");
        return;
    }
    for message in messages {
        println!("{} [{}]", message.id, message.conversation_id);
        println!("sender={}", message.sender_actor_id);
        println!("sender_device={}", message.sender_device_id);
        if let Some(recipient) = message.recipient_actor_id.as_deref() {
            println!("recipient={recipient}");
        }
        println!("delivery_ids={}", message.delivery_ids.join(","));
        if let Some(created_at) = message.created_at.as_deref() {
            println!("created_at={created_at}");
        }
        println!();
    }
}

fn print_owner_e2ee_devices(devices: &[OwnerE2eeDevice]) {
    if devices.is_empty() {
        println!("No E2EE devices found");
        return;
    }
    for device in devices {
        println!(
            "{} [{}] {}",
            device.device_id, device.status, device.protocol
        );
        println!("actor={}", device.actor_id);
        if let Some(display_name) = device.display_name.as_deref() {
            println!("display_name={display_name}");
        }
        println!("fingerprint={}", device.fingerprint);
        if let Some(updated_at) = device.updated_at.as_deref() {
            println!("updated_at={updated_at}");
        }
        println!();
    }
}

fn print_owner_e2ee_peer_devices(devices: &[OwnerE2eePeerDevice]) {
    if devices.is_empty() {
        println!("No E2EE peer devices found");
        return;
    }
    for device in devices {
        println!(
            "{} [{}] {}",
            device.device_id, device.trust_state, device.protocol
        );
        println!("actor={}", device.actor_id);
        if let Some(display_name) = device.display_name.as_deref() {
            println!("display_name={display_name}");
        }
        println!("fingerprint={}", device.fingerprint);
        if let Some(last_seen_at) = device.last_seen_at.as_deref() {
            println!("last_seen_at={last_seen_at}");
        }
        println!();
    }
}

fn print_owner_search(results: &OwnerSearchResult) {
    let guard = &results.public_search_guard;
    if guard.blocked || guard.requires_confirmation || !guard.categories.is_empty() {
        println!(
            "public_search_guard=blocked:{} requires_confirmation:{} confirmed:{}",
            guard.blocked, guard.requires_confirmation, guard.confirmed
        );
        if !guard.categories.is_empty() {
            println!("public_search_categories={}", guard.categories.join(","));
        }
        if let Some(message) = guard.message.as_deref() {
            println!("public_search_message={message}");
        }
    }
    println!("posts={}", results.posts.len());
    for post in &results.posts {
        println!(
            "{} [{}] {} {}",
            post.id,
            post.visibility.as_deref().unwrap_or("unknown"),
            post.protocol.as_deref().unwrap_or("activitypub"),
            post.published_at.as_deref().unwrap_or("")
        );
        println!(
            "audience={}",
            audience_description(post.visibility.as_deref().unwrap_or("unknown"))
        );
        println!("{}", post.content);
        println!();
    }
    println!("users={}", results.users.len());
    for user in &results.users {
        println!(
            "{} [{}] {} {}",
            user.actor_id,
            user.relation,
            user.status,
            user.created_at.as_deref().unwrap_or("")
        );
    }
    println!("sources={}", results.sources.len());
    for source in &results.sources {
        println!(
            "{} [{}] {} {}",
            source.id,
            source.source_type,
            source.status,
            source.title.as_deref().unwrap_or(&source.url)
        );
        println!("url={}", source.url);
        if let Some(homepage) = source.homepage_url.as_deref() {
            println!("homepage={homepage}");
        }
    }
    println!("source_items={}", results.source_items.len());
    for item in &results.source_items {
        println!(
            "{} [{}] {} {}",
            item.id,
            item.source_type,
            if owner_source_item_read(&item.read) {
                "read"
            } else {
                "unread"
            },
            item.published_at.as_deref().unwrap_or("")
        );
        println!("{}", item.title);
        if let Some(url) = item.canonical_url.as_deref() {
            println!("url={url}");
        }
        if let Some(excerpt) = item.excerpt.as_deref() {
            println!("{excerpt}");
        }
        println!();
    }
    println!("public_posts={}", results.public_posts.len());
    for post in &results.public_posts {
        println!(
            "{} [{}] {} {}",
            post.id,
            post.network,
            post.provider,
            post.published_at.as_deref().unwrap_or("")
        );
        if let Some(handle) = post.actor_handle.as_deref() {
            println!("author={handle}");
        }
        println!("url={}", post.url);
        if let Some(watch_type) = post.watch_type.as_deref() {
            println!("watch_type={watch_type}");
        }
        if let Some(watch_target) = post.watch_target.as_deref() {
            println!("watch_target={watch_target}");
        }
        if let Some(reply_target) = post.reply_target.as_deref() {
            println!("reply_target={reply_target}");
        }
        if !post.actions.is_empty() {
            println!("actions={}", post.actions.join(","));
        }
        println!("{}", post.content);
        println!();
    }
    println!("public_actors={}", results.public_actors.len());
    for actor in &results.public_actors {
        println!(
            "{} [{}] {} {}",
            actor.id,
            actor.network,
            actor.provider,
            actor.handle.as_deref().unwrap_or("")
        );
        if let Some(display_name) = actor.display_name.as_deref() {
            println!("name={display_name}");
        }
        if let Some(url) = actor.url.as_deref() {
            println!("url={url}");
        }
        if let Some(watch_type) = actor.watch_type.as_deref() {
            println!("watch_type={watch_type}");
        }
        if let Some(watch_target) = actor.watch_target.as_deref() {
            println!("watch_target={watch_target}");
        }
        if let Some(follow_target) = actor.follow_target.as_deref() {
            println!("follow_target={follow_target}");
        }
        if !actor.actions.is_empty() {
            println!("actions={}", actor.actions.join(","));
        }
    }
    if !results.provider_errors.is_empty() {
        println!("provider_errors={}", results.provider_errors.len());
        for error in &results.provider_errors {
            println!("{} [{}] {}", error.provider, error.network, error.error);
        }
    }
}

fn owner_source_item_read(value: &serde_json::Value) -> bool {
    value == &serde_json::Value::Bool(true)
        || value == &serde_json::Value::Number(1.into())
        || value == &serde_json::Value::String("1".to_string())
        || value == &serde_json::Value::String("true".to_string())
}

fn print_owner_stats(stats: &OwnerStats) {
    println!("followers_total={}", stats.followers_total);
    println!("followers_approved={}", stats.followers_approved);
    println!("followers_pending={}", stats.followers_pending);
    println!("followers_rejected={}", stats.followers_rejected);
    println!("following_total={}", stats.following_total);
    println!("posts_total={}", stats.posts_total);
    println!("posts_public={}", stats.public_posts);
    println!("posts_private={}", stats.private_posts);
    println!("posts_direct={}", stats.direct_posts);
    println!("posts_encrypted={}", stats.encrypted_posts);
    println!("posts_media={}", stats.media_posts);
    println!("posts_dual_protocol={}", stats.dual_protocol_posts);
    println!("activities_total={}", stats.activities_total);
    println!("deliveries_total={}", stats.deliveries_total);
    println!("deliveries_queued={}", stats.deliveries_queued);
    println!("deliveries_retry={}", stats.deliveries_retry);
    println!("deliveries_delivered={}", stats.deliveries_delivered);
    println!("deliveries_failed={}", stats.deliveries_failed);
    println!("notifications_unread={}", stats.notifications_unread);
    println!("blocks_total={}", stats.blocks_total);
    println!("allowlist_hosts={}", stats.allowlist_hosts);
    println!("closed_network={}", stats.closed_network);
}

fn print_owner_diagnostics(diagnostics: &[DiagnosticStatus]) {
    if diagnostics.is_empty() {
        println!("No diagnostics found");
        return;
    }
    for diagnostic in diagnostics {
        println!(
            "{}={} {}",
            diagnostic.key,
            if diagnostic.ok { "ok" } else { "warn" },
            diagnostic.detail
        );
    }
}

fn print_owner_sources(sources: &OwnerSources) {
    println!("subscriptions={}", sources.subscriptions.len());
    for source in &sources.subscriptions {
        println!(
            "{} [{}] {} cadence={}m errors={}",
            source.id,
            source.status,
            source.source_type,
            source.refresh_cadence_minutes,
            source.error_count
        );
        println!("url={}", source.url);
        if let Some(title) = source.title.as_deref() {
            println!("title={title}");
        }
        if let Some(last_error) = source.last_error.as_deref() {
            println!("error={last_error}");
        }
        println!();
    }
    println!("items={}", sources.items.len());
    for item in sources.items.iter().take(20) {
        println!(
            "{} [{}] {}",
            item.id,
            item.source_type,
            if item.read { "read" } else { "unread" }
        );
        println!("{}", item.title);
        if let Some(url) = item.canonical_url.as_deref() {
            println!("url={url}");
        }
        println!();
    }
}

fn print_owner_moderation(moderation: &ModerationState) {
    println!("closed_network={}", moderation.closed_network);
    println!("blocks={}", moderation.block_count);
    for block in &moderation.blocks {
        println!("{}", block.id);
        println!("actor={}", block.actor_id);
        if let Some(domain) = block.blocked_domain.as_deref() {
            println!("domain={domain}");
        }
        if let Some(reason) = block.reason.as_deref() {
            println!("reason={reason}");
        }
        println!("created={}", block.created_at.as_deref().unwrap_or(""));
        println!();
    }
    println!("allowlist={}", moderation.allowlist_count);
    for host in &moderation.allowlist {
        println!("{} enabled={}", host.host, host.enabled);
        if let Some(note) = host.note.as_deref() {
            println!("note={note}");
        }
        println!("updated={}", host.updated_at.as_deref().unwrap_or(""));
        println!();
    }
}

fn print_owner_discovered_actor(actor: &OwnerDiscoveredActor) {
    println!(
        "actor={}",
        actor
            .handle
            .as_deref()
            .or(actor.name.as_deref())
            .unwrap_or(&actor.id)
    );
    println!("id={}", actor.id);
    if let Some(name) = actor.name.as_deref() {
        println!("name={name}");
    }
    if let Some(username) = actor.preferred_username.as_deref() {
        println!("preferred_username={username}");
    }
    if let Some(actor_type) = actor.actor_type.as_deref() {
        println!("actor_type={actor_type}");
    }
    if let Some(summary) = actor.summary.as_deref() {
        println!("summary={summary}");
    }
    if let Some(url) = actor.url.as_deref() {
        println!("url={url}");
    }
    if let Some(icon_url) = actor.icon_url.as_deref() {
        println!("icon={icon_url}");
    }
    println!("inbox={}", actor.inbox);
    if let Some(shared_inbox) = actor.shared_inbox.as_deref() {
        println!("shared_inbox={shared_inbox}");
    }
    println!(
        "following_status={}",
        actor.following_status.as_deref().unwrap_or("not-following")
    );
    if let Some(post) = actor.target_public_post.as_ref() {
        println!(
            "target_public_post={}",
            post.url.as_deref().unwrap_or(&post.id)
        );
        println!("target_public_post_type={}", post.kind);
        if let Some(actor_id) = post.actor_id.as_deref() {
            println!("target_public_post_actor={actor_id}");
        }
        if let Some(published) = post.published.as_deref() {
            println!("target_public_post_published={published}");
        }
        if !post.content.is_empty() {
            println!("target_public_post_content={}", post.content);
        }
    }
    if !actor.recent_public_posts.is_empty() {
        println!("recent_public_posts={}", actor.recent_public_posts.len());
        for post in &actor.recent_public_posts {
            println!();
            println!("post={}", post.url.as_deref().unwrap_or(&post.id));
            println!("type={}", post.kind);
            if let Some(actor_id) = post.actor_id.as_deref() {
                println!("actor={actor_id}");
            }
            if let Some(name) = post.name.as_deref() {
                println!("name={name}");
            }
            if let Some(published) = post.published.as_deref() {
                println!("published={published}");
            }
            if !post.content.is_empty() {
                println!("content={}", post.content);
            }
        }
    }
}

fn print_owner_post_detail(detail: &OwnerPostDetail) {
    println!("id={}", detail.post.id);
    println!("visibility={:?}", detail.post.visibility);
    println!(
        "audience={}",
        audience_description(&format!("{:?}", detail.post.visibility))
    );
    println!("protocol={:?}", detail.post.protocol);
    if let Some(reply_to) = detail.in_reply_to.as_deref() {
        println!("reply_to={reply_to}");
    }
    println!(
        "published_at={}",
        detail.post.published_at.as_deref().unwrap_or("")
    );
    println!("attachments={}", detail.post.attachments.len());
    println!(
        "replies={} likes={} boosts={}",
        detail.post.reply_count, detail.post.like_count, detail.post.boost_count
    );
    println!("{}", detail.post.content);
}

fn audience_description(visibility: &str) -> &'static str {
    match visibility.to_ascii_lowercase().as_str() {
        "public" => {
            "PUBLIC: visible on public web, public ActivityPub/Mastodon surfaces, and enabled public protocol routes"
        }
        "unlisted" => {
            "UNLISTED: reachable by URL but kept out of public listing surfaces where supported"
        }
        "followers" | "private" => {
            "FRIENDS/FOLLOWERS: visible to approved followers; not in anonymous public feeds"
        }
        "direct" => "DIRECT: intended only for named recipients; not for general friends or public feeds",
        _ => "UNKNOWN: verify the post detail before assuming who can see it",
    }
}

async fn handle_notifications(command: NotificationsCommand) -> Result<()> {
    match command {
        NotificationsCommand::List { limit, remote } => {
            let db = D1Client::new(remote)?;
            let notifications = db.list_notifications(limit).await?;
            output::print_notifications(&notifications);
        }
        NotificationsCommand::Read { id, remote } => {
            let db = D1Client::new(remote)?;
            db.mark_notification_read(&id).await?;
            println!("Marked notification {id} read");
        }
    }

    Ok(())
}

async fn handle_deliveries(command: DeliveriesCommand) -> Result<()> {
    match command {
        DeliveriesCommand::List(args) => {
            let db = D1Client::new(args.remote)?;
            let deliveries = db
                .list_deliveries(args.limit, args.status.as_deref())
                .await?;
            output::print_deliveries(&deliveries);
        }
        DeliveriesCommand::Enqueue(args) => {
            let report = delivery::enqueue_delivery(&args.base_url, &args.id).await?;
            output::print_delivery_enqueue_report(&report);
        }
        DeliveriesCommand::Process(args) => {
            let report =
                delivery::process_delivery(&args.base_url, args.admin_token.as_deref(), &args.id)
                    .await?;
            output::print_delivery_process_report(&report);
        }
        DeliveriesCommand::ProcessQueued(args) => {
            let status = args.status.trim().to_ascii_lowercase();
            if status != "queued" && status != "retry" {
                anyhow::bail!("process-queued only supports --status queued or retry");
            }

            let db = D1Client::new(args.remote)?;
            let deliveries = db.list_deliveries(args.limit, Some(&status)).await?;
            if deliveries.is_empty() {
                println!("No {status} deliveries found");
                return Ok(());
            }

            for delivery in deliveries {
                let report = delivery::process_delivery(
                    &args.base_url,
                    args.admin_token.as_deref(),
                    &delivery.id,
                )
                .await?;
                output::print_delivery_process_report(&report);
            }
        }
    }

    Ok(())
}

async fn handle_timeline(command: cli::TopLevelTimelineCommand, store: &ConfigStore) -> Result<()> {
    match command {
        cli::TopLevelTimelineCommand::Home {
            limit,
            protocol,
            remote,
            before,
        } => match protocol {
            Protocol::Atproto => {
                let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;
                let feed = client.get_timeline(limit).await?;
                output::print_feed(&feed.feed);
            }
            Protocol::ActivityPub => {
                let db = D1Client::new(remote)?;
                let posts = db.home_timeline(limit, before.as_deref()).await?;
                output::print_timeline(&posts);
            }
            Protocol::Both => {
                let db = D1Client::new(remote)?;
                let posts = db.home_timeline(limit, before.as_deref()).await?;
                output::print_timeline(&posts);

                if posts.is_empty() {
                    let mut client = atproto::AtprotoClient::from_config(&store.load_bluesky()?)?;
                    let feed = client.get_timeline(limit).await?;
                    output::print_feed(&feed.feed);
                }
            }
        },
    }

    Ok(())
}

fn prompt_password() -> Result<String> {
    use std::io::{self, Write};

    print!("Password or app password: ");
    io::stdout().flush()?;
    let mut password = String::new();
    io::stdin().read_line(&mut password)?;
    Ok(password.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audience(id: &str, members: &[&str]) -> OwnerAudienceList {
        OwnerAudienceList {
            id: id.to_string(),
            name: "Close Friends".to_string(),
            description: None,
            allowed_categories: Vec::new(),
            member_actor_ids: members.iter().map(|member| member.to_string()).collect(),
            member_count: members.len() as u64,
            created_at: None,
            updated_at: None,
        }
    }

    fn peer(actor_id: &str, device_id: &str, trust_state: &str) -> OwnerE2eePeerDevice {
        OwnerE2eePeerDevice {
            id: format!("{actor_id}#{device_id}"),
            actor_id: actor_id.to_string(),
            device_id: device_id.to_string(),
            display_name: None,
            protocol: "dais-mls-v1".to_string(),
            credential: format!(
                "-----BEGIN PUBLIC KEY-----\n{actor_id}-{device_id}\n-----END PUBLIC KEY-----"
            ),
            key_package: "test-key-package".to_string(),
            fingerprint: format!("fingerprint-{device_id}"),
            trust_state: trust_state.to_string(),
            first_seen_at: None,
            last_seen_at: None,
            trusted_at: None,
            revoked_at: None,
        }
    }

    #[test]
    fn group_e2ee_plan_tracks_audience_membership_changes() {
        let alice = "https://alice.example/users/alice";
        let bob = "https://bob.example/users/bob";
        let peers = vec![
            peer(alice, "phone", "trusted"),
            peer(bob, "laptop", "trusted"),
        ];

        let original = plan_owner_e2ee_group_recipients(
            &audience("close-friends", &[alice, bob]),
            &peers,
            false,
        )
        .unwrap();
        let after_remove =
            plan_owner_e2ee_group_recipients(&audience("close-friends", &[alice]), &peers, false)
                .unwrap();

        assert_eq!(original.member_count, 2);
        assert_eq!(original.deliveries.len(), 2);
        assert_eq!(original.recipients.len(), 2);
        assert_eq!(after_remove.member_count, 1);
        assert_eq!(after_remove.deliveries.len(), 1);
        assert_eq!(after_remove.recipients.len(), 1);
        assert!(after_remove
            .recipients
            .contains_key(&owner_e2ee_group_key_id(alice, "phone")));
        assert!(!after_remove
            .recipients
            .contains_key(&owner_e2ee_group_key_id(bob, "laptop")));
    }

    #[test]
    fn group_e2ee_plan_requires_trusted_devices_by_default() {
        let alice = "https://alice.example/users/alice";
        let peers = vec![peer(alice, "phone", "untrusted")];

        assert!(plan_owner_e2ee_group_recipients(
            &audience("close-friends", &[alice]),
            &peers,
            false
        )
        .is_err());
        assert!(plan_owner_e2ee_group_recipients(
            &audience("close-friends", &[alice]),
            &peers,
            true
        )
        .is_ok());
    }

    #[test]
    fn recovery_key_inventory_is_scoped_to_instance() {
        let root =
            std::env::temp_dir().join(format!("dais-e2ee-recovery-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = ConfigStore::new(root.clone());
        store
            .save_e2ee_private_key("https://social.dais.social", "dais-device", "key", false)
            .unwrap();
        store
            .save_e2ee_private_key("https://social.skpt.cl", "skpt-device", "key", false)
            .unwrap();

        let dais_keys = local_e2ee_key_device_ids(&store, "https://social.dais.social").unwrap();

        assert!(dais_keys.contains("dais-device"));
        assert!(!dais_keys.contains("skpt-device"));
        let _ = std::fs::remove_dir_all(root);
    }
}
