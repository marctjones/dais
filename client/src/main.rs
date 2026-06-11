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
use clap::{CommandFactory, Parser};
use cli::{
    ActorsCommand, BlueskyCommand, Cli, Command, DeliveriesCommand, E2eeCommand, EventsCommand,
    FollowCommand, FollowersCommand, FriendsCommand, MediaCommand, ModerationCommand,
    NotificationsCommand, PostCommand, ReportsCommand, SearchCommand, TimelineCommand,
};
use config::ConfigStore;
use d1::D1Client;
use posting::{
    delete_activitypub_post, publish_interaction, publish_post, update_activitypub_post,
    ActivityOutcome, PostDraft, PostOutcome,
};
use rand::RngCore;
use routing::Protocol;
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
                    read_url: _,
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
