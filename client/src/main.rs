mod atproto;
mod cli;
mod config;
mod d1;
mod delivery;
mod doctor;
mod e2ee;
mod output;
mod posting;
mod routing;
mod tui;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use cli::{
    BlueskyCommand, Cli, Command, DeliveriesCommand, E2eeCommand, FollowCommand, FollowersCommand,
    FriendsCommand, NotificationsCommand, PostCommand, SearchCommand, TimelineCommand,
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
