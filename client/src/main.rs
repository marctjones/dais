mod atproto;
mod cli;
mod config;
mod d1;
mod e2ee;
mod output;
mod posting;
mod routing;
mod tui;

use anyhow::Result;
use clap::Parser;
use cli::{
    BlueskyCommand, Cli, Command, E2eeCommand, FollowCommand, FollowersCommand, FriendsCommand,
    PostCommand, SearchCommand, TimelineCommand,
};
use config::ConfigStore;
use d1::D1Client;
use posting::{publish_post, PostDraft, PostOutcome};
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
        Command::E2ee(command) => handle_e2ee(command).await?,
        Command::Tui(args) => tui::run(args.remote, &store).await?,
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
            let db = D1Client::new(args.remote)?;
            let draft = PostDraft::from_create_args(cli::CreatePostArgs {
                text: args.text,
                visibility: args.visibility,
                public: args.public,
                protocol: args.protocol,
                encrypt: args.encrypt,
                recipients: args.recipients,
                reply_to: args.reply_to,
                to: args.to,
                remote: args.remote,
            })?;

            let result = publish_post(draft, store, &db).await?;
            match result {
                PostOutcome::ActivityPub {
                    post_id,
                    read_url,
                    delivery_ids,
                } => {
                    if encrypt {
                        println!("Encrypted ActivityPub post stored");
                        println!("Post: {post_id}");
                        if let Some(read_url) = read_url {
                            println!("Read URL: {read_url}");
                        }
                        println!("No decryption key was included in the fallback link.");
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
