//! `dais` — the operator CLI. Noun-verb command map over the `dais-client` SDK
//! (CLIENT_REDESIGN.md §4). Human output by default, `--json` opt-in, confirmations
//! on outward/irreversible actions.

mod output;

use std::io::{IsTerminal, Read, Write};

use anyhow::{anyhow, Result};
use clap::{Args, Parser, Subcommand};
use dais_client::model::{Feed, Visibility};
use dais_client::{Client, Config};

use output::Style;

#[derive(Parser)]
#[command(
    name = "dais",
    version,
    about = "Operator client for your single-user fediverse instance (CLI + TUI)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize config and the local store
    Init(InitArgs),
    /// Show client + instance status
    Status,
    /// Read feeds (home, mentions, sent, a user)
    #[command(subcommand)]
    Timeline(TimelineCmd),
    /// Compose a post (text, or - for stdin)
    Post(PostArgs),
    /// Show a post and its replies
    Thread(ThreadArgs),
    /// Who you follow
    #[command(subcommand)]
    Follow(FollowCmd),
    /// Incoming follow requests (the approval inbox)
    #[command(subcommand)]
    Requests(RequestsCmd),
    /// Your followers
    #[command(subcommand)]
    Followers(FollowersCmd),
    /// Mutual follows
    #[command(subcommand)]
    Friends(FriendsCmd),
    /// Direct messages
    #[command(subcommand)]
    Dm(DmCmd),
    /// Notifications
    #[command(subcommand)]
    Notify(NotifyCmd),
    /// Blocks
    #[command(subcommand)]
    Block(BlockCmd),
    /// Your profile
    #[command(subcommand)]
    Account(AccountCmd),
    /// Launch the TUI
    Tui,
}

#[derive(Args)]
struct InitArgs {
    #[arg(long)]
    handle: Option<String>,
    #[arg(long)]
    instance: Option<String>,
    /// Seed the local store with the design-doc sample feed
    #[arg(long)]
    demo: bool,
}

#[derive(Args, Clone)]
struct FeedArgs {
    #[arg(long)]
    json: bool,
    #[arg(long, default_value_t = 50)]
    limit: usize,
}

#[derive(Subcommand)]
enum TimelineCmd {
    /// Your home timeline (ingested from inbox, #63)
    Home(FeedArgs),
    /// Posts mentioning you
    Mentions(FeedArgs),
    /// Posts you've sent
    Sent(FeedArgs),
    /// A specific user's posts
    User {
        handle: String,
        #[command(flatten)]
        feed: FeedArgs,
    },
}

#[derive(Args)]
struct PostArgs {
    /// Post text, or `-` to read stdin
    text: Option<String>,
    #[arg(long, short)]
    visibility: Option<String>,
    #[arg(long)]
    encrypt: bool,
    #[arg(long)]
    reply: Option<String>,
    /// Skip confirmation for public posts
    #[arg(long)]
    yes: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ThreadArgs {
    id: String,
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum FollowCmd {
    Add { handle: String },
    List,
    Remove { handle: String },
}

#[derive(Subcommand)]
enum RequestsCmd {
    List {
        #[arg(long)]
        json: bool,
    },
    Approve {
        handle: String,
        #[arg(long)]
        yes: bool,
    },
    Reject {
        handle: String,
    },
}

#[derive(Subcommand)]
enum FollowersCmd {
    List,
    Remove { handle: String },
}

#[derive(Subcommand)]
enum FriendsCmd {
    List {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum DmCmd {
    Send {
        handle: String,
        text: String,
        #[arg(long)]
        encrypt: bool,
    },
    List,
    Read {
        handle: String,
    },
}

#[derive(Subcommand)]
enum NotifyCmd {
    List,
    Read {
        id: Option<String>,
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand)]
enum BlockCmd {
    Add { target: String },
    List,
    Remove { target: String },
}

#[derive(Subcommand)]
enum AccountCmd {
    Show {
        #[arg(long)]
        json: bool,
    },
    Edit,
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{}", Style::new().yellow(&format!("error: {e}")));
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init(args) => cmd_init(args),
        Command::Status => cmd_status().await,
        Command::Timeline(cmd) => cmd_timeline(cmd),
        Command::Post(args) => cmd_post(args).await,
        Command::Thread(args) => cmd_thread(args),
        Command::Requests(cmd) => cmd_requests(cmd).await,
        Command::Friends(cmd) => cmd_friends(cmd),
        Command::Account(cmd) => cmd_account(cmd),
        Command::Follow(cmd) => cmd_follow(cmd).await,
        Command::Followers(cmd) => cmd_followers(cmd).await,
        Command::Dm(cmd) => cmd_dm(cmd).await,
        Command::Notify(cmd) => cmd_notify(cmd).await,
        Command::Block(cmd) => cmd_block(cmd).await,
        Command::Tui => {
            if !std::io::stdout().is_terminal() {
                return Err(anyhow!("the TUI needs an interactive terminal (stdout is not a TTY)"));
            }
            let client = Client::open()?;
            dais_tui::run(client)
        }
    }
}

fn cmd_init(args: InitArgs) -> Result<()> {
    let s = Style::new();
    let mut cfg = Config::load().unwrap_or_default();
    if let Some(h) = args.handle {
        cfg.handle = Some(h);
    }
    if let Some(i) = args.instance {
        cfg.instance = Some(i);
    }
    // Derive instance from handle if not given (@user@domain → domain).
    if cfg.instance.is_none() {
        if let Some(h) = &cfg.handle {
            if let Some(domain) = h.trim_start_matches('@').split('@').nth(1) {
                cfg.instance = Some(domain.to_string());
            }
        }
    }
    cfg.save()?;

    let client = Client::with_config(cfg)?;
    if args.demo {
        client.seed_demo()?;
    }

    println!("{}", s.bold("dais initialized."));
    println!("  config: {}", Config::config_path()?.display());
    println!("  store:  {}", Config::store_path()?.display());
    if let Some(h) = &client.config.handle {
        println!("  handle: {}", s.cyan(h));
    } else {
        println!(
            "  {}",
            s.dim("no handle set — `dais init --handle @you@your.domain`")
        );
    }
    if args.demo {
        println!("  {}", s.green("seeded demo feed — try `dais timeline home` or `dais tui`"));
    }
    Ok(())
}

async fn cmd_status() -> Result<()> {
    let s = Style::new();
    let client = Client::open()?;
    println!("{}", s.bold("dais client status"));
    println!(
        "  handle:   {}",
        client
            .config
            .handle
            .clone()
            .map(|h| s.cyan(&h))
            .unwrap_or_else(|| s.dim("(unset)"))
    );
    println!(
        "  instance: {}",
        client
            .config
            .instance
            .clone()
            .unwrap_or_else(|| s.dim("(unset)").to_string())
    );
    println!("  config:   {}", Config::config_path()?.display());
    println!("  store:    {}", Config::store_path()?.display());

    for (label, feed) in [("home", Feed::Home), ("mentions", Feed::Mentions), ("sent", Feed::Sent)] {
        let n = client.store.unread_count(feed).unwrap_or(0);
        let total = client.store.timeline(feed, 100_000).map(|v| v.len()).unwrap_or(0);
        println!("  {label:<9} {total} posts, {n} unread");
    }

    // Key
    match client.signer() {
        Ok(sg) => println!("  signing:  {} ({})", s.green("configured"), sg.key_id()),
        Err(_) => println!("  signing:  {}", s.dim("not configured")),
    }

    // D1 connectivity
    if client.config.d1.is_complete() {
        match client.d1() {
            Ok(d1) => match d1.ping().await {
                Ok(()) => println!("  D1:       {}", s.green("reachable")),
                Err(e) => println!("  D1:       {} ({e})", s.yellow("error")),
            },
            Err(e) => println!("  D1:       {} ({e})", s.yellow("error")),
        }
    } else {
        println!("  D1:       {}", s.dim("not configured (offline / local store only)"));
    }
    Ok(())
}

fn cmd_timeline(cmd: TimelineCmd) -> Result<()> {
    let client = Client::open()?;
    let (feed, fa, user) = match cmd {
        TimelineCmd::Home(fa) => (Feed::Home, fa, None),
        TimelineCmd::Mentions(fa) => (Feed::Mentions, fa, None),
        TimelineCmd::Sent(fa) => (Feed::Sent, fa, None),
        TimelineCmd::User { handle, feed } => (Feed::Home, feed, Some(handle)),
    };
    if let Some(h) = user {
        return Err(anyhow!(
            "timeline user {h}: not wired yet — needs remote fetch (#80 later phase)"
        ));
    }
    let posts = client.timeline(feed, fa.limit)?;
    if fa.json {
        let arr: Vec<_> = posts.iter().map(output::post_json).collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else if posts.is_empty() {
        println!(
            "{}",
            Style::new().dim("(empty — try `dais init --demo`, or this feed has no posts yet)")
        );
    } else {
        for p in &posts {
            output::print_post(p);
        }
    }
    Ok(())
}

async fn cmd_post(args: PostArgs) -> Result<()> {
    let s = Style::new();
    let client = Client::open()?;

    let text = match args.text.as_deref() {
        Some("-") | None if !std::io::stdin().is_terminal() => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf.trim_end().to_string()
        }
        Some("-") => return Err(anyhow!("nothing on stdin")),
        Some(t) => t.to_string(),
        None => return Err(anyhow!("provide post text, or pipe it via stdin and pass -")),
    };
    if text.is_empty() {
        return Err(anyhow!("empty post"));
    }

    let visibility = match args.visibility.as_deref() {
        Some(v) => Visibility::parse(v)
            .ok_or_else(|| anyhow!("unknown visibility '{v}' (public|followers|direct)"))?,
        None => client.config.default_visibility(),
    };

    if visibility == Visibility::Public
        && !args.yes
        && !confirm(&s.yellow("This posts PUBLICLY to the whole fediverse. Continue?"), args.yes)?
    {
        println!("{}", s.dim("cancelled"));
        return Ok(());
    }

    // Offline / unconfigured: stage locally (no wire delivery).
    if !client.can_federate() {
        let res = client.compose(&text, visibility, args.encrypt, args.reply.as_deref())?;
        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "staged_draft_id": res.draft_id,
                    "visibility": res.visibility.label(),
                    "encrypted": res.encrypt,
                    "delivered": false,
                }))?
            );
        } else {
            println!(
                "{} draft #{} · {} {}{}",
                s.yellow("staged (offline)"),
                res.draft_id,
                visibility.glyph(),
                visibility.label(),
                if res.encrypt { s.magenta(" · 🔒") } else { String::new() }
            );
            println!("{}", s.dim("not delivered — configure D1 + key (`dais init`) to federate"));
        }
        return Ok(());
    }

    let out = client
        .publish(&text, visibility, args.encrypt, args.reply.as_deref())
        .await?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "post_id": out.post_id,
                "visibility": visibility.label(),
                "encrypted": args.encrypt,
                "followers_targeted": out.followers_targeted,
                "delivered": out.delivered,
                "failed": out.failed,
            }))?
        );
        return Ok(());
    }

    println!(
        "{} · {} {}{}",
        s.green("posted"),
        visibility.glyph(),
        visibility.label(),
        if args.encrypt { s.magenta(" · 🔒 encrypted") } else { String::new() }
    );
    println!("  {}", s.dim(&out.post_id));
    println!(
        "  delivered to {}/{} follower inbox(es){}",
        out.delivered,
        out.followers_targeted,
        if out.failed > 0 { format!(", {} failed", out.failed) } else { String::new() }
    );
    Ok(())
}

fn cmd_thread(args: ThreadArgs) -> Result<()> {
    let client = Client::open()?;
    let (root, replies) = client.thread(&args.id)?;
    let root = root.ok_or_else(|| anyhow!("no post with id {}", args.id))?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "post": output::post_json(&root),
                "replies": replies.iter().map(output::post_json).collect::<Vec<_>>(),
            }))?
        );
        return Ok(());
    }
    output::print_post(&root);
    let s = Style::new();
    println!("{}", s.dim(&format!("— {} replies —", replies.len())));
    println!();
    for r in &replies {
        output::print_post(r);
    }
    Ok(())
}

async fn cmd_requests(cmd: RequestsCmd) -> Result<()> {
    let s = Style::new();
    let client = Client::open()?;

    // When configured for prod, operate on real D1 + deliver Accept/Reject.
    if client.can_federate() {
        match cmd {
            RequestsCmd::List { json } => {
                let reqs = client.requests_remote().await?;
                if json {
                    let arr: Vec<_> = reqs
                        .iter()
                        .map(|r| {
                            serde_json::json!({
                                "follower": r.follower_actor_id,
                                "inbox": r.follower_inbox,
                                "created_at": r.created_at,
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&arr)?);
                } else if reqs.is_empty() {
                    println!("{}", s.dim("no pending follow requests"));
                } else {
                    for r in &reqs {
                        println!("{} {}  {}", s.cyan("●"), s.bold(&r.follower_actor_id), s.dim(&r.created_at));
                    }
                }
            }
            RequestsCmd::Approve { handle, yes } => {
                if !confirm(
                    &format!("Approve {handle}? They'll be able to read your followers-only posts."),
                    yes,
                )? {
                    println!("{}", s.dim("cancelled"));
                    return Ok(());
                }
                client.request_approve(&handle).await?;
                println!("{} {handle} {}", s.green("approved"), s.dim("(Accept delivered)"));
            }
            RequestsCmd::Reject { handle } => {
                client.request_reject(&handle).await?;
                println!("{} {handle} {}", s.yellow("rejected"), s.dim("(Reject delivered)"));
            }
        }
        return Ok(());
    }

    // Offline / demo: operate on the local store.
    match cmd {
        RequestsCmd::List { json } => {
            let reqs = client.requests()?;
            if json {
                let arr: Vec<_> = reqs
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "handle": r.handle,
                            "name": r.name,
                            "message": r.message,
                            "asked_at": r.asked_at.to_rfc3339(),
                            "mutuals": r.mutuals,
                            "unread": r.unread,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&arr)?);
            } else if reqs.is_empty() {
                println!("{}", s.dim("no pending follow requests"));
            } else {
                for r in &reqs {
                    let dot = if r.unread { "●" } else { "○" };
                    println!(
                        "{} {}  {}  {}",
                        s.cyan(dot),
                        s.bold(r.name.as_deref().unwrap_or(&r.handle)),
                        s.dim(&r.handle),
                        s.dim(&format!("asked {} ago", dais_client::relative_time(r.asked_at)))
                    );
                    if let Some(m) = &r.message {
                        println!("    {}", m);
                    }
                    println!(
                        "    {}",
                        s.dim(&format!(
                            "{} mutuals · {} posts",
                            r.mutuals,
                            r.post_count.map(|c| c.to_string()).unwrap_or_else(|| "?".into())
                        ))
                    );
                    println!();
                }
            }
        }
        RequestsCmd::Approve { handle, yes } => {
            if !confirm(
                &format!("Approve {handle}? They'll be able to read your followers-only posts."),
                yes,
            )? {
                println!("{}", s.dim("cancelled"));
                return Ok(());
            }
            // Local bookkeeping; outbound Accept lands with the worker wiring.
            if client.store.remove_request(&handle)? {
                println!("{} {handle} {}", s.green("approved"), s.dim("(local; Accept delivery is a later phase)"));
            } else {
                return Err(anyhow!("no pending request from {handle}"));
            }
        }
        RequestsCmd::Reject { handle } => {
            if client.store.remove_request(&handle)? {
                println!("{} {handle}", s.yellow("rejected"));
            } else {
                return Err(anyhow!("no pending request from {handle}"));
            }
        }
    }
    Ok(())
}

fn cmd_friends(cmd: FriendsCmd) -> Result<()> {
    let client = Client::open()?;
    let FriendsCmd::List { json } = cmd;
    // Approximate friends from the local store: distinct authors flagged is_friend
    // (#64's `friends` view is server-side; this is the client-visible subset).
    let mut handles: Vec<(String, Option<String>)> = Vec::new();
    for p in client.timeline(Feed::Home, 100_000)? {
        if p.is_friend && !handles.iter().any(|(h, _)| h == &p.author_handle) {
            handles.push((p.author_handle.clone(), p.author_name.clone()));
        }
    }
    if json {
        let arr: Vec<_> = handles
            .iter()
            .map(|(h, n)| serde_json::json!({ "handle": h, "name": n }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else if handles.is_empty() {
        println!("{}", Style::new().dim("no friends (mutual follows) visible in the local store yet"));
    } else {
        let s = Style::new();
        for (h, n) in &handles {
            println!("{} {}  {}", s.yellow("★"), s.bold(n.as_deref().unwrap_or(h)), s.dim(h));
        }
    }
    Ok(())
}

fn cmd_account(cmd: AccountCmd) -> Result<()> {
    let client = Client::open()?;
    match cmd {
        AccountCmd::Show { json } => {
            let handle = client.config.handle.clone().unwrap_or_default();
            let instance = client.config.instance.clone().unwrap_or_default();
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "handle": handle, "instance": instance,
                    }))?
                );
            } else {
                let s = Style::new();
                println!("{} {}", s.bold("handle:  "), s.cyan(&handle));
                println!("{} {}", s.bold("instance:"), instance);
            }
            Ok(())
        }
        AccountCmd::Edit => not_wired("account edit"),
    }
}

fn require_federation(client: &Client) -> Result<()> {
    if !client.can_federate() {
        return Err(anyhow!(
            "this needs D1 + a signing key — run `dais init` and set d1.* and keys.* in config.toml"
        ));
    }
    Ok(())
}

async fn cmd_follow(cmd: FollowCmd) -> Result<()> {
    let s = Style::new();
    let client = Client::open()?;
    require_federation(&client)?;
    match cmd {
        FollowCmd::Add { handle } => {
            let target = client.follow_add(&handle).await?;
            println!("{} {handle} {}", s.green("follow sent"), s.dim(&format!("→ {target}")));
        }
        FollowCmd::List => {
            let rows = client.following_list().await?;
            if rows.is_empty() {
                println!("{}", s.dim("not following anyone yet"));
            } else {
                for (actor, status) in rows {
                    let st = if status == "accepted" { s.green(&status) } else { s.dim(&status) };
                    println!("{}  {}", s.bold(&actor), st);
                }
            }
        }
        FollowCmd::Remove { handle } => {
            client.follow_remove(&handle).await?;
            println!("{} {handle} {}", s.yellow("unfollowed"), s.dim("(Undo delivered)"));
        }
    }
    Ok(())
}

async fn cmd_followers(cmd: FollowersCmd) -> Result<()> {
    let s = Style::new();
    let client = Client::open()?;
    require_federation(&client)?;
    match cmd {
        FollowersCmd::List => {
            let rows = client.followers_list().await?;
            if rows.is_empty() {
                println!("{}", s.dim("no approved followers yet"));
            } else {
                for actor in rows {
                    println!("{}", actor);
                }
            }
        }
        FollowersCmd::Remove { handle } => {
            client.follower_remove(&handle).await?;
            println!("{} {handle}", s.yellow("removed follower"));
        }
    }
    Ok(())
}

async fn cmd_dm(cmd: DmCmd) -> Result<()> {
    let s = Style::new();
    let client = Client::open()?;
    require_federation(&client)?;
    match cmd {
        DmCmd::Send { handle, text, encrypt } => {
            let id = client.dm_send(&handle, &text, encrypt).await?;
            println!(
                "{} → {handle}{}",
                s.green("DM sent"),
                if encrypt { s.magenta(" · 🔒 encrypted") } else { String::new() }
            );
            println!("  {}", s.dim(&id));
        }
        DmCmd::List => {
            let convos = client.dm_list().await?;
            if convos.is_empty() {
                println!("{}", s.dim("no conversations"));
            } else {
                for (id, last) in convos {
                    println!("{}  {}", s.bold(&id), s.dim(&last));
                }
            }
        }
        DmCmd::Read { handle } => {
            let msgs = client.dm_thread(&handle).await?;
            if msgs.is_empty() {
                println!("{}", s.dim("no messages"));
            } else {
                for (sender, content, when) in msgs {
                    println!("{} {}", s.cyan(&sender), s.dim(&when));
                    println!("  {content}");
                }
            }
        }
    }
    Ok(())
}

async fn cmd_notify(cmd: NotifyCmd) -> Result<()> {
    let s = Style::new();
    let client = Client::open()?;
    require_federation(&client)?;
    match cmd {
        NotifyCmd::List => {
            let rows = client.notify_list(50).await?;
            if rows.is_empty() {
                println!("{}", s.dim("no notifications"));
            } else {
                for (typ, who, when) in rows {
                    println!("{} {}  {}", s.cyan(&format!("[{typ}]")), s.bold(&who), s.dim(&when));
                }
            }
        }
        NotifyCmd::Read { id, all } => {
            if !all && id.is_none() {
                return Err(anyhow!("specify a notification id or --all"));
            }
            client.notify_mark_read(if all { None } else { id.as_deref() }).await?;
            println!("{}", s.green(if all { "marked all read" } else { "marked read" }));
        }
    }
    Ok(())
}

async fn cmd_block(cmd: BlockCmd) -> Result<()> {
    let s = Style::new();
    let client = Client::open()?;
    require_federation(&client)?;
    match cmd {
        BlockCmd::Add { target } => {
            client.block_add(&target).await?;
            println!("{} {target}", s.yellow("blocked"));
        }
        BlockCmd::List => {
            let rows = client.block_list().await?;
            if rows.is_empty() {
                println!("{}", s.dim("no blocks"));
            } else {
                for t in rows {
                    println!("{t}");
                }
            }
        }
        BlockCmd::Remove { target } => {
            client.block_remove(&target).await?;
            println!("{} {target}", s.green("unblocked"));
        }
    }
    Ok(())
}

fn not_wired(what: &str) -> Result<()> {
    let s = Style::new();
    println!(
        "{}",
        s.yellow(&format!(
            "‘{what}’ isn’t wired yet — it needs a server/worker endpoint (client redesign #80, later phase). Work is tracked under epic #70."
        ))
    );
    Ok(())
}

fn confirm(prompt: &str, yes: bool) -> Result<bool> {
    if yes {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        return Err(anyhow!("refusing without --yes in a non-interactive shell"));
    }
    print!("{prompt} [y/N] ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}

