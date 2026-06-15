use std::collections::{HashMap, HashSet};
use std::io;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
    Frame, Terminal,
};

use crate::atproto::AtprotoClient;
use crate::config::ConfigStore;
use crate::d1::{
    D1Block, D1Client, D1Delivery, D1DirectMessage, D1Friend, D1Notification, D1Post, D1SourceItem,
    D1TimelinePost, D1User, ServerStats,
};
use crate::posting::{publish_post, PostDraft, PostOutcome};
use crate::routing::{Protocol, Visibility};
use dais_client_core::{
    OwnerApiClient, OwnerDiscoveredActor, OwnerFollower, OwnerFollowing, OwnerInteraction,
    OwnerPostDetail, OwnerProfile, OwnerTimelinePost,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
enum Tab {
    Reader,
    Discovery,
    Home,
    Posts,
    Friends,
    Followers,
    Following,
    Notifications,
    Profile,
    Deliveries,
    DMs,
    Search,
    Bluesky,
    Sources,
    Stats,
    Blocks,
}

impl Tab {
    fn all() -> [Tab; 16] {
        [
            Tab::Reader,
            Tab::Discovery,
            Tab::Home,
            Tab::Posts,
            Tab::Friends,
            Tab::Followers,
            Tab::Following,
            Tab::Notifications,
            Tab::Profile,
            Tab::Deliveries,
            Tab::DMs,
            Tab::Search,
            Tab::Bluesky,
            Tab::Sources,
            Tab::Stats,
            Tab::Blocks,
        ]
    }

    fn title(self) -> &'static str {
        match self {
            Tab::Reader => "Reader",
            Tab::Discovery => "Discovery",
            Tab::Home => "Home",
            Tab::Posts => "Posts",
            Tab::Friends => "Friends",
            Tab::Followers => "Followers",
            Tab::Following => "Following",
            Tab::Notifications => "Notifications",
            Tab::Profile => "Profile",
            Tab::Deliveries => "Deliveries",
            Tab::DMs => "DMs",
            Tab::Search => "Search",
            Tab::Bluesky => "Bluesky",
            Tab::Sources => "Sources",
            Tab::Stats => "Stats",
            Tab::Blocks => "Blocks",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Normal,
    Compose,
    Search,
    Discovery,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ComposeField {
    Body,
    Recipients,
    DirectRecipients,
    ReplyTo,
}

#[derive(Clone, Debug)]
struct TextBuffer {
    lines: Vec<String>,
    row: usize,
    col: usize,
}

impl TextBuffer {
    fn new() -> Self {
        Self {
            lines: vec![String::new()],
            row: 0,
            col: 0,
        }
    }

    fn from_text(value: &str) -> Self {
        let mut lines: Vec<String> = value.lines().map(|line| line.to_string()).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        let row = lines.len().saturating_sub(1);
        let col = lines[row].chars().count();
        Self { lines, row, col }
    }

    fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.row = 0;
        self.col = 0;
    }

    fn text(&self) -> String {
        self.lines.join("\n")
    }

    fn insert_char(&mut self, ch: char) {
        let line = self.lines.get_mut(self.row).expect("valid row");
        let idx = char_to_byte_idx(line, self.col);
        line.insert(idx, ch);
        self.col += 1;
    }

    fn insert_newline(&mut self) {
        let line = self.lines.get_mut(self.row).expect("valid row");
        let idx = char_to_byte_idx(line, self.col);
        let tail = line.split_off(idx);
        self.lines.insert(self.row + 1, tail);
        self.row += 1;
        self.col = 0;
    }

    fn backspace(&mut self) {
        if self.col > 0 {
            let line = self.lines.get_mut(self.row).expect("valid row");
            let idx = char_to_byte_idx(line, self.col - 1);
            line.remove(idx);
            self.col -= 1;
            return;
        }

        if self.row > 0 {
            let current = self.lines.remove(self.row);
            self.row -= 1;
            let prev = self.lines.get_mut(self.row).expect("valid row");
            self.col = prev.chars().count();
            prev.push_str(&current);
        }
    }

    fn move_left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.lines[self.row].chars().count();
        }
    }

    fn move_right(&mut self) {
        let current_len = self.lines[self.row].chars().count();
        if self.col < current_len {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    fn move_up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.col = self.col.min(self.lines[self.row].chars().count());
        }
    }

    fn move_down(&mut self) {
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = self.col.min(self.lines[self.row].chars().count());
        }
    }
}

fn char_to_byte_idx(value: &str, char_idx: usize) -> usize {
    value
        .char_indices()
        .nth(char_idx)
        .map(|(idx, _)| idx)
        .unwrap_or_else(|| value.len())
}

#[derive(Clone, Debug)]
struct ComposeState {
    body: TextBuffer,
    recipients: TextBuffer,
    direct_recipients: TextBuffer,
    reply_to: TextBuffer,
    visibility: Visibility,
    protocol: Protocol,
    encrypt: bool,
    field: ComposeField,
}

impl ComposeState {
    fn new() -> Self {
        Self {
            body: TextBuffer::new(),
            recipients: TextBuffer::from_text(""),
            direct_recipients: TextBuffer::from_text(""),
            reply_to: TextBuffer::from_text(""),
            visibility: Visibility::Followers,
            protocol: Protocol::ActivityPub,
            encrypt: false,
            field: ComposeField::Body,
        }
    }

    fn reset(&mut self) {
        self.body.clear();
        self.recipients.clear();
        self.direct_recipients.clear();
        self.reply_to.clear();
        self.visibility = Visibility::Followers;
        self.protocol = Protocol::ActivityPub;
        self.encrypt = false;
        self.field = ComposeField::Body;
    }
}

#[derive(Clone, Debug)]
struct SearchState {
    query: TextBuffer,
}

impl SearchState {
    fn new() -> Self {
        Self {
            query: TextBuffer::new(),
        }
    }
}

#[derive(Clone, Debug)]
enum TabData {
    Reader(Vec<OwnerTimelinePost>),
    ReaderDetail(OwnerPostDetail),
    Discovery(OwnerDiscoveredActor),
    Home(Vec<D1TimelinePost>),
    Posts(Vec<D1Post>),
    Friends(Vec<D1Friend>),
    Followers(Vec<OwnerFollower>),
    Following(Vec<OwnerFollowing>),
    Notifications(Vec<D1Notification>),
    Profile(OwnerProfile),
    Deliveries(Vec<D1Delivery>),
    DMs(Vec<D1DirectMessage>),
    Search {
        posts: Vec<D1Post>,
        users: Vec<D1User>,
        query: String,
    },
    Bluesky(Vec<crate::atproto::FeedItem>),
    Sources(Vec<D1SourceItem>),
    Stats(ServerStats),
    Blocks(Vec<D1Block>),
}

#[derive(Clone, Debug)]
struct Entry {
    title: String,
    subtitle: String,
    details: String,
}

enum Message {
    Loaded(Tab, TabData),
    Error(String),
    Status(String),
    Refresh(Tab),
}

pub async fn run(remote: bool, store: &ConfigStore) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(remote, store.clone(), tx.clone());

    app.refresh(Tab::Home);
    app.refresh(Tab::Stats);

    let result = loop {
        terminal.draw(|frame| app.draw(frame))?;

        while let Ok(message) = rx.try_recv() {
            app.apply(message);
        }

        if app.quit {
            break Ok(());
        }

        if event::poll(Duration::from_millis(120))? {
            match event::read()? {
                Event::Key(key) => app.handle_key(key),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

struct App {
    remote: bool,
    store: ConfigStore,
    tx: tokio::sync::mpsc::UnboundedSender<Message>,
    active: Tab,
    selection: HashMap<Tab, usize>,
    loading: HashSet<Tab>,
    quit: bool,
    status: String,
    mode: Mode,
    compose: ComposeState,
    search: SearchState,
    discovery: SearchState,
    home: Vec<D1TimelinePost>,
    posts: Vec<D1Post>,
    friends: Vec<crate::d1::D1Friend>,
    followers: Vec<OwnerFollower>,
    following: Vec<OwnerFollowing>,
    notifications: Vec<D1Notification>,
    profile: Option<OwnerProfile>,
    deliveries: Vec<D1Delivery>,
    direct_messages: Vec<D1DirectMessage>,
    search_posts: Vec<D1Post>,
    search_users: Vec<D1User>,
    reader: Vec<OwnerTimelinePost>,
    reader_details: HashMap<String, OwnerPostDetail>,
    discovered_actor: Option<OwnerDiscoveredActor>,
    bluesky_feed: Vec<crate::atproto::FeedItem>,
    source_items: Vec<D1SourceItem>,
    stats: Option<ServerStats>,
    blocks: Vec<D1Block>,
}

impl App {
    fn new(
        remote: bool,
        store: ConfigStore,
        tx: tokio::sync::mpsc::UnboundedSender<Message>,
    ) -> Self {
        let mut selection = HashMap::new();
        for tab in Tab::all() {
            selection.insert(tab, 0);
        }

        Self {
            remote,
            store,
            tx,
            active: Tab::Home,
            selection,
            loading: HashSet::new(),
            quit: false,
            status: "Ready".to_string(),
            mode: Mode::Normal,
            compose: ComposeState::new(),
            search: SearchState::new(),
            discovery: SearchState::new(),
            home: Vec::new(),
            posts: Vec::new(),
            friends: Vec::new(),
            followers: Vec::new(),
            following: Vec::new(),
            notifications: Vec::new(),
            profile: None,
            deliveries: Vec::new(),
            direct_messages: Vec::new(),
            search_posts: Vec::new(),
            search_users: Vec::new(),
            reader: Vec::new(),
            reader_details: HashMap::new(),
            discovered_actor: None,
            bluesky_feed: Vec::new(),
            source_items: Vec::new(),
            stats: None,
            blocks: Vec::new(),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Compose => self.handle_compose_key(key),
            Mode::Search => self.handle_search_key(key),
            Mode::Discovery => self.handle_discovery_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Right | KeyCode::Tab => self.next_tab(),
            KeyCode::Left if key.modifiers.contains(KeyModifiers::SHIFT) => self.prev_tab(),
            KeyCode::Left => self.prev_tab(),
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::Char('r') => self.refresh(self.active),
            KeyCode::Char('c') => {
                self.compose.reset();
                self.mode = Mode::Compose;
                self.status = "Compose private post".to_string();
            }
            KeyCode::Char('d') => {
                self.compose.reset();
                self.compose.visibility = Visibility::Direct;
                self.compose.protocol = Protocol::ActivityPub;
                self.compose.field = ComposeField::DirectRecipients;
                self.mode = Mode::Compose;
                self.status = "Compose direct message".to_string();
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.status = "Search".to_string();
            }
            KeyCode::Char('g') => {
                self.mode = Mode::Discovery;
                self.active = Tab::Discovery;
                self.status = "Discover actor".to_string();
            }
            KeyCode::Char('f') => self.follow_discovered_actor(),
            KeyCode::Char('w') => self.unfollow_selected_following(),
            KeyCode::Char('a') => self.approve_selected_follower(),
            KeyCode::Char('x') => self.reject_selected_follower(),
            KeyCode::Char('m') => self.mark_selected_notification_read(),
            KeyCode::Char('u') => self.unblock_selected_block(),
            KeyCode::Enter => self.load_selected_reader_detail(),
            KeyCode::Char('l') => self.reader_interaction("like"),
            KeyCode::Char('o') => self.reader_interaction("boost"),
            KeyCode::Char('y') => self.reply_to_selected_reader_post(),
            KeyCode::Char('b') => {
                self.compose.reset();
                self.compose.visibility = Visibility::Public;
                self.compose.protocol = Protocol::Both;
                self.mode = Mode::Compose;
                self.status = "Compose public post".to_string();
            }
            KeyCode::Char('e') => {
                self.compose.reset();
                self.compose.encrypt = true;
                self.mode = Mode::Compose;
                self.status = "Compose encrypted post".to_string();
            }
            _ => {}
        }
    }

    fn handle_compose_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.compose.reset();
                self.status = "Compose canceled".to_string();
            }
            KeyCode::Tab => {
                self.compose.field = match self.compose.field {
                    ComposeField::Body => ComposeField::Recipients,
                    ComposeField::Recipients => ComposeField::DirectRecipients,
                    ComposeField::DirectRecipients => ComposeField::ReplyTo,
                    ComposeField::ReplyTo => ComposeField::Body,
                };
            }
            KeyCode::BackTab => {
                self.compose.field = match self.compose.field {
                    ComposeField::Body => ComposeField::ReplyTo,
                    ComposeField::Recipients => ComposeField::Body,
                    ComposeField::DirectRecipients => ComposeField::Recipients,
                    ComposeField::ReplyTo => ComposeField::DirectRecipients,
                };
            }
            KeyCode::Char('v') => {
                self.compose.visibility = match self.compose.visibility {
                    Visibility::Followers => Visibility::Public,
                    Visibility::Public => Visibility::Unlisted,
                    Visibility::Unlisted => Visibility::Direct,
                    Visibility::Direct => Visibility::Followers,
                };
            }
            KeyCode::Char('p') => {
                self.compose.protocol = match self.compose.protocol {
                    Protocol::ActivityPub => Protocol::Atproto,
                    Protocol::Atproto => Protocol::Both,
                    Protocol::Both => Protocol::ActivityPub,
                };
            }
            KeyCode::Char('e') => self.compose.encrypt = !self.compose.encrypt,
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.send_compose();
            }
            KeyCode::Enter => {
                if self.compose.field != ComposeField::ReplyTo {
                    self.current_compose_buffer().insert_newline();
                }
            }
            KeyCode::Backspace => {
                self.current_compose_buffer().backspace();
            }
            KeyCode::Left => self.current_compose_buffer().move_left(),
            KeyCode::Right => self.current_compose_buffer().move_right(),
            KeyCode::Up => self.current_compose_buffer().move_up(),
            KeyCode::Down => self.current_compose_buffer().move_down(),
            KeyCode::Char(ch) => self.current_compose_buffer().insert_char(ch),
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status = "Search canceled".to_string();
            }
            KeyCode::Enter => {
                let query = self.search.query.text();
                if query.trim().is_empty() {
                    self.status = "Search query is empty".to_string();
                } else {
                    self.run_search(query.clone());
                    self.active = Tab::Search;
                    self.mode = Mode::Normal;
                    self.status = format!("Searching for {query}");
                }
            }
            KeyCode::Backspace => self.search.query.backspace(),
            KeyCode::Left => self.search.query.move_left(),
            KeyCode::Right => self.search.query.move_right(),
            KeyCode::Up => self.search.query.move_up(),
            KeyCode::Down => self.search.query.move_down(),
            KeyCode::Char(ch) => self.search.query.insert_char(ch),
            _ => {}
        }
    }

    fn handle_discovery_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status = "Discovery canceled".to_string();
            }
            KeyCode::Enter => {
                let target = self.discovery.query.text();
                if target.trim().is_empty() {
                    self.status = "Discovery target is empty".to_string();
                } else {
                    self.run_discovery(target.clone());
                    self.active = Tab::Discovery;
                    self.mode = Mode::Normal;
                    self.status = format!("Resolving {target}");
                }
            }
            KeyCode::Backspace => self.discovery.query.backspace(),
            KeyCode::Left => self.discovery.query.move_left(),
            KeyCode::Right => self.discovery.query.move_right(),
            KeyCode::Up => self.discovery.query.move_up(),
            KeyCode::Down => self.discovery.query.move_down(),
            KeyCode::Char(ch) => self.discovery.query.insert_char(ch),
            _ => {}
        }
    }

    fn current_compose_buffer(&mut self) -> &mut TextBuffer {
        match self.compose.field {
            ComposeField::Body => &mut self.compose.body,
            ComposeField::Recipients => &mut self.compose.recipients,
            ComposeField::DirectRecipients => &mut self.compose.direct_recipients,
            ComposeField::ReplyTo => &mut self.compose.reply_to,
        }
    }

    fn next_tab(&mut self) {
        let tabs = Tab::all();
        let idx = tabs.iter().position(|tab| *tab == self.active).unwrap_or(0);
        self.active = tabs[(idx + 1) % tabs.len()];
        self.ensure_loaded(self.active);
    }

    fn prev_tab(&mut self) {
        let tabs = Tab::all();
        let idx = tabs.iter().position(|tab| *tab == self.active).unwrap_or(0);
        self.active = tabs[(idx + tabs.len() - 1) % tabs.len()];
        self.ensure_loaded(self.active);
    }

    fn move_selection(&mut self, delta: isize) {
        let count = self.entries(self.active).len();
        if count == 0 {
            return;
        }
        let current = *self.selection.get(&self.active).unwrap_or(&0);
        let next = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize)
        };
        self.selection
            .insert(self.active, next.min(count.saturating_sub(1)));
    }

    fn refresh(&mut self, tab: Tab) {
        self.status = format!("Refreshing {}", tab.title());
        let tx = self.tx.clone();
        let store = self.store.clone();
        let remote = self.remote;
        self.loading.insert(tab);

        tokio::spawn(async move {
            let result = load_tab(remote, store, tab).await;
            match result {
                Ok(data) => {
                    let _ = tx.send(Message::Loaded(tab, data));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn ensure_loaded(&mut self, tab: Tab) {
        if tab == Tab::Search || tab == Tab::Discovery {
            return;
        }
        if self.loading.contains(&tab) {
            return;
        }
        if self.entries(tab).is_empty() {
            self.refresh(tab);
        }
    }

    fn run_search(&mut self, query: String) {
        self.loading.insert(Tab::Search);
        self.status = format!("Searching for {query}");
        let tx = self.tx.clone();
        let store = self.store.clone();
        let remote = self.remote;

        tokio::spawn(async move {
            let result = load_search(remote, store, &query).await;
            match result {
                Ok((posts, users)) => {
                    let _ = tx.send(Message::Loaded(
                        Tab::Search,
                        TabData::Search {
                            posts,
                            users,
                            query,
                        },
                    ));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn run_discovery(&mut self, target: String) {
        self.loading.insert(Tab::Discovery);
        self.status = format!("Resolving {target}");
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let client = match owner_api_from_env() {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match client.discover_actor(&target).await {
                Ok(actor) => {
                    let label = actor
                        .handle
                        .clone()
                        .or(actor.name.clone())
                        .unwrap_or_else(|| actor.id.clone());
                    let _ = tx.send(Message::Loaded(Tab::Discovery, TabData::Discovery(actor)));
                    let _ = tx.send(Message::Status(format!("Resolved {label}")));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn send_compose(&mut self) {
        let text = self.compose.body.text();
        if text.trim().is_empty() {
            self.status = "Cannot send an empty post".to_string();
            return;
        }

        let mut recipients = HashMap::new();
        for line in self.compose.recipients.text().lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let (key_id, path) = match line.split_once('=') {
                Some(parts) => parts,
                None => {
                    self.status = "Recipients must be key_id=public_key_pem_file".to_string();
                    return;
                }
            };
            match std::fs::read_to_string(path) {
                Ok(value) => {
                    recipients.insert(key_id.to_string(), value);
                }
                Err(error) => {
                    self.status = format!("Could not read recipient key {path}: {error}");
                    return;
                }
            }
        }
        let to: Vec<String> = self
            .compose
            .direct_recipients
            .text()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        if self.compose.visibility == Visibility::Direct && to.is_empty() {
            self.status = "Direct posts need at least one actor URL".to_string();
            return;
        }
        let reply_to = self.compose.reply_to.text().trim().to_string();
        let reply_to = if reply_to.is_empty() {
            None
        } else {
            Some(reply_to)
        };

        let draft = PostDraft {
            text,
            visibility: self.compose.visibility,
            protocol: self.compose.protocol,
            encrypt: self.compose.encrypt,
            recipients: recipients.into_iter().collect(),
            reply_to,
            to,
            e2ee_fallback: crate::cli::E2eeFallbackMode::Strict,
            object_type: crate::cli::ActivityObjectType::Note,
            title: None,
            summary: None,
            starts_at: None,
            ends_at: None,
            location: None,
            poll_options: Vec::new(),
            poll_multiple: false,
            attachments: Vec::new(),
        };

        let tx = self.tx.clone();
        let store = self.store.clone();
        let remote = self.remote;
        let db = match D1Client::new(remote) {
            Ok(db) => db,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };

        self.status = "Publishing post".to_string();
        self.mode = Mode::Normal;
        self.compose.reset();
        self.loading.insert(Tab::Posts);
        self.loading.insert(Tab::Home);

        tokio::spawn(async move {
            let outcome = publish_post(draft, &store, &db).await;
            match outcome {
                Ok(result) => {
                    let (status, refresh_tabs) = match result {
                        PostOutcome::ActivityPub {
                            post_id,
                            read_url,
                            delivery_ids,
                            ..
                        } => {
                            let mut status = format!("Published ActivityPub post {post_id}");
                            if let Some(read_url) = read_url {
                                status.push_str(&format!(" (read: {read_url})"));
                            }
                            status.push_str(&format!("; deliveries queued {}", delivery_ids.len()));
                            (status, vec![Tab::Home, Tab::Posts])
                        }
                        PostOutcome::Bluesky { uri } => {
                            (format!("Published Bluesky post {uri}"), vec![Tab::Bluesky])
                        }
                        PostOutcome::Both {
                            post_id,
                            uri,
                            read_url,
                            delivery_ids,
                            ..
                        } => {
                            let mut status = format!(
                                "Published ActivityPub post {post_id} and Bluesky post {uri}"
                            );
                            if let Some(read_url) = read_url {
                                status.push_str(&format!(" (read: {read_url})"));
                            }
                            status.push_str(&format!("; deliveries queued {}", delivery_ids.len()));
                            (status, vec![Tab::Home, Tab::Posts, Tab::Bluesky])
                        }
                    };
                    let _ = tx.send(Message::Status(status));
                    for tab in refresh_tabs {
                        let _ = tx.send(Message::Refresh(tab));
                    }
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn approve_selected_follower(&mut self) {
        let Some(row) = self.followers.get(self.selected(Tab::Followers)) else {
            return;
        };
        let follower = row.follower_actor_id.clone();
        let tx = self.tx.clone();
        self.status = format!("Approving {follower}");
        tokio::spawn(async move {
            let client = match owner_api_from_env() {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match client.set_follower_status(&follower, "approved").await {
                Ok(_) => {
                    let _ = tx.send(Message::Status(format!("Approved {follower}")));
                    let _ = tx.send(Message::Refresh(Tab::Followers));
                    let _ = tx.send(Message::Refresh(Tab::Friends));
                    let _ = tx.send(Message::Refresh(Tab::Stats));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn reject_selected_follower(&mut self) {
        let Some(row) = self.followers.get(self.selected(Tab::Followers)) else {
            return;
        };
        let follower = row.follower_actor_id.clone();
        let tx = self.tx.clone();
        self.status = format!("Rejecting {follower}");
        tokio::spawn(async move {
            let client = match owner_api_from_env() {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match client.set_follower_status(&follower, "rejected").await {
                Ok(_) => {
                    let _ = tx.send(Message::Status(format!("Rejected {follower}")));
                    let _ = tx.send(Message::Refresh(Tab::Followers));
                    let _ = tx.send(Message::Refresh(Tab::Stats));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn mark_selected_notification_read(&mut self) {
        let Some(row) = self.notifications.get(self.selected(Tab::Notifications)) else {
            return;
        };
        let id = row.id.clone();
        let tx = self.tx.clone();
        let remote = self.remote;
        tokio::spawn(async move {
            let db = match D1Client::new(remote) {
                Ok(db) => db,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match db.mark_notification_read(&id).await {
                Ok(()) => {
                    let _ = tx.send(Message::Status(format!("Marked notification {id} read")));
                    let _ = tx.send(Message::Refresh(Tab::Notifications));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn unblock_selected_block(&mut self) {
        let Some(row) = self.blocks.get(self.selected(Tab::Blocks)) else {
            return;
        };
        let actor_id = row.actor_id.clone();
        let tx = self.tx.clone();
        let remote = self.remote;
        tokio::spawn(async move {
            let db = match D1Client::new(remote) {
                Ok(db) => db,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match db.unblock(&actor_id).await {
                Ok(()) => {
                    let _ = tx.send(Message::Status(format!("Unblocked {actor_id}")));
                    let _ = tx.send(Message::Refresh(Tab::Blocks));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn selected_reader_object_id(&self) -> Option<String> {
        self.reader
            .get(self.selected(Tab::Reader))
            .map(|row| row.object_id.clone())
    }

    fn load_selected_reader_detail(&mut self) {
        if self.active != Tab::Reader {
            return;
        }
        let Some(object_id) = self.selected_reader_object_id() else {
            return;
        };
        let tx = self.tx.clone();
        self.loading.insert(Tab::Reader);
        self.status = format!("Loading detail {object_id}");
        tokio::spawn(async move {
            let client = match owner_api_from_env() {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match client.post_detail(&object_id).await {
                Ok(detail) => {
                    let _ = tx.send(Message::Loaded(Tab::Reader, TabData::ReaderDetail(detail)));
                    let _ = tx.send(Message::Status(format!("Loaded detail {object_id}")));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn reader_interaction(&mut self, interaction: &'static str) {
        if self.active != Tab::Reader {
            return;
        }
        let Some(object_id) = self.selected_reader_object_id() else {
            return;
        };
        let tx = self.tx.clone();
        self.status = format!("Sending {interaction} for {object_id}");
        tokio::spawn(async move {
            let client = match owner_api_from_env() {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            let action = OwnerInteraction {
                object_id: object_id.clone(),
                interaction: interaction.to_string(),
            };
            match client.interact(&action).await {
                Ok(_) => {
                    let _ = tx.send(Message::Status(format!(
                        "{interaction} queued for {object_id}"
                    )));
                    let _ = tx.send(Message::Refresh(Tab::Reader));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn reply_to_selected_reader_post(&mut self) {
        if self.active != Tab::Reader {
            return;
        }
        let Some(object_id) = self.selected_reader_object_id() else {
            return;
        };
        self.compose.reset();
        self.compose.visibility = Visibility::Public;
        self.compose.protocol = Protocol::ActivityPub;
        self.compose.reply_to = TextBuffer::from_text(&object_id);
        self.mode = Mode::Compose;
        self.status = format!("Replying to {object_id}");
    }

    fn follow_discovered_actor(&mut self) {
        if self.active != Tab::Discovery {
            return;
        }
        let Some(actor) = self.discovered_actor.clone() else {
            self.status = "Resolve an actor before following".to_string();
            return;
        };
        let target = actor.id;
        let tx = self.tx.clone();
        self.status = format!("Following {target}");
        tokio::spawn(async move {
            let client = match owner_api_from_env() {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match client.follow_actor(&target).await {
                Ok(result) => {
                    let _ = tx.send(Message::Status(format!(
                        "Follow requested for {}",
                        result.following.target_actor_id
                    )));
                    let _ = tx.send(Message::Refresh(Tab::Following));
                    let _ = tx.send(Message::Refresh(Tab::Reader));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn unfollow_selected_following(&mut self) {
        if self.active != Tab::Following {
            return;
        }
        let Some(row) = self.following.get(self.selected(Tab::Following)) else {
            self.status = "Select a followed actor before unfollowing".to_string();
            return;
        };
        let target = row.target_actor_id.clone();
        let tx = self.tx.clone();
        self.status = format!("Unfollowing {target}");
        tokio::spawn(async move {
            let client = match owner_api_from_env() {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match client.unfollow_actor(&target).await {
                Ok(result) => {
                    let _ = tx.send(Message::Status(format!(
                        "Unfollow requested for {}",
                        result.following.target_actor_id
                    )));
                    let _ = tx.send(Message::Refresh(Tab::Following));
                    let _ = tx.send(Message::Refresh(Tab::Reader));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn selected(&self, tab: Tab) -> usize {
        *self.selection.get(&tab).unwrap_or(&0)
    }

    fn apply(&mut self, message: Message) {
        match message {
            Message::Loaded(tab, data) => {
                self.loading.remove(&tab);
                match data {
                    TabData::Reader(value) => self.reader = value,
                    TabData::ReaderDetail(value) => {
                        self.reader_details.insert(value.post.id.clone(), value);
                    }
                    TabData::Discovery(value) => self.discovered_actor = Some(value),
                    TabData::Home(value) => self.home = value,
                    TabData::Posts(value) => self.posts = value,
                    TabData::Friends(value) => self.friends = value,
                    TabData::Followers(value) => self.followers = value,
                    TabData::Following(value) => self.following = value,
                    TabData::Notifications(value) => self.notifications = value,
                    TabData::Profile(value) => self.profile = Some(value),
                    TabData::Deliveries(value) => self.deliveries = value,
                    TabData::DMs(value) => self.direct_messages = value,
                    TabData::Search {
                        posts,
                        users,
                        query,
                    } => {
                        self.search_posts = posts;
                        self.search_users = users;
                        self.search.query = TextBuffer::from_text(&query);
                    }
                    TabData::Bluesky(value) => self.bluesky_feed = value,
                    TabData::Sources(value) => self.source_items = value,
                    TabData::Stats(value) => self.stats = Some(value),
                    TabData::Blocks(value) => self.blocks = value,
                }
                self.status = format!("Loaded {}", tab.title());
            }
            Message::Error(error) => {
                self.loading.clear();
                self.status = error;
            }
            Message::Status(status) => {
                self.status = status;
            }
            Message::Refresh(tab) => {
                self.refresh(tab);
            }
        }
    }

    fn draw(&self, frame: &mut Frame<'_>) {
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(frame.area());

        self.draw_tabs(frame, areas[0]);
        self.draw_body(frame, areas[1]);
        self.draw_footer(frame, areas[2]);

        if self.mode == Mode::Compose {
            self.draw_compose_overlay(frame);
        } else if self.mode == Mode::Search {
            self.draw_search_overlay(frame);
        } else if self.mode == Mode::Discovery {
            self.draw_discovery_overlay(frame);
        }
    }

    fn draw_tabs(&self, frame: &mut Frame<'_>, area: Rect) {
        let titles: Vec<Line> = Tab::all()
            .iter()
            .map(|tab| {
                Line::from(Span::styled(
                    tab.title(),
                    Style::default().fg(if *tab == self.active {
                        Color::Black
                    } else {
                        Color::Gray
                    }),
                ))
            })
            .collect();

        let tabs = Tabs::new(titles)
            .select(self.active_index())
            .block(Block::default().borders(Borders::ALL).title("dais"))
            .style(Style::default().fg(Color::Gray))
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );
        frame.render_widget(tabs, area);
    }

    fn active_index(&self) -> usize {
        Tab::all()
            .iter()
            .position(|tab| *tab == self.active)
            .unwrap_or(0)
    }

    fn draw_body(&self, frame: &mut Frame<'_>, area: Rect) {
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
            .split(area);
        let entries = self.entries(self.active);

        let items: Vec<ListItem> = entries
            .iter()
            .map(|entry| {
                ListItem::new(vec![
                    Line::from(entry.title.clone()),
                    Line::from(Span::styled(
                        entry.subtitle.clone(),
                        Style::default().fg(Color::DarkGray),
                    )),
                ])
            })
            .collect();

        let mut state = ratatui::widgets::ListState::default();
        if !entries.is_empty() {
            state.select(Some(self.selected(self.active).min(entries.len() - 1)));
        }

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(format!(
                "{} ({})",
                self.active.title(),
                entries.len()
            )))
            .highlight_style(
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");
        frame.render_stateful_widget(list, layout[0], &mut state);

        let selected = self
            .selected(self.active)
            .min(entries.len().saturating_sub(1));
        let detail = if let Some(entry) = entries.get(selected) {
            entry.details.clone()
        } else {
            self.empty_detail(self.active)
        };
        let detail_block = Paragraph::new(detail)
            .block(Block::default().borders(Borders::ALL).title("Details"))
            .wrap(Wrap { trim: false });
        frame.render_widget(detail_block, layout[1]);
    }

    fn draw_footer(&self, frame: &mut Frame<'_>, area: Rect) {
        let left = match self.mode {
            Mode::Normal => "q quit · tab switch · r refresh · g discover · f follow · w unfollow · enter detail · y/l/o actions",
            Mode::Compose => "ctrl+s send · tab field · v visibility · p protocol · e e2ee · esc cancel",
            Mode::Search => "enter search · esc cancel · backspace edit",
            Mode::Discovery => "enter lookup actor · esc cancel · backspace edit",
        };
        let right = if self.loading.contains(&self.active) {
            "Loading..."
        } else {
            self.status.as_str()
        };

        let footer = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
            .split(area);
        frame.render_widget(
            Paragraph::new(left).block(Block::default().borders(Borders::ALL)),
            footer[0],
        );
        frame.render_widget(
            Paragraph::new(right).block(Block::default().borders(Borders::ALL)),
            footer[1],
        );
    }

    fn draw_compose_overlay(&self, frame: &mut Frame<'_>) {
        let area = centered_rect(80, 84, frame.area());
        let block = Block::default().borders(Borders::ALL).title("Compose");
        frame.render_widget(block, area);
        let inner = area.inner(Margin::new(1, 1));
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Min(8),
                Constraint::Length(5),
            ])
            .split(inner);

        let meta = Paragraph::new(vec![
            Line::from(format!("Audience: {}", self.compose.visibility)),
            Line::from(format!(
                "Protocol: {}",
                match self.compose.protocol {
                    Protocol::ActivityPub => "activitypub",
                    Protocol::Atproto => "atproto",
                    Protocol::Both => "both",
                }
            )),
            Line::from(format!(
                "Encrypt: {}",
                if self.compose.encrypt { "on" } else { "off" }
            )),
        ])
        .block(Block::default().borders(Borders::ALL).title("Options"));
        frame.render_widget(meta, layout[0]);

        let editor_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(64), Constraint::Percentage(36)])
            .split(layout[1]);
        let body = Paragraph::new(self.compose.body.text())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(compose_field_title(
                        "Body",
                        self.compose.field == ComposeField::Body,
                    )),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(body, editor_chunks[0]);

        let recipient_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(38),
                Constraint::Percentage(38),
                Constraint::Percentage(24),
            ])
            .split(editor_chunks[1]);
        let e2ee_recipients = Paragraph::new(self.compose.recipients.text())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(compose_field_title(
                        "E2EE keys",
                        self.compose.field == ComposeField::Recipients,
                    )),
            )
            .wrap(Wrap { trim: false });
        let direct_recipients = Paragraph::new(self.compose.direct_recipients.text())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(compose_field_title(
                        "Direct actor URLs",
                        self.compose.field == ComposeField::DirectRecipients,
                    )),
            )
            .wrap(Wrap { trim: false });
        let reply_to = Paragraph::new(self.compose.reply_to.text())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(compose_field_title(
                        "Reply to URL",
                        self.compose.field == ComposeField::ReplyTo,
                    )),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(e2ee_recipients, recipient_chunks[0]);
        frame.render_widget(direct_recipients, recipient_chunks[1]);
        frame.render_widget(reply_to, recipient_chunks[2]);

        let instructions = Paragraph::new(
            "E2EE keys use key_id=public_key_pem_file. Direct messages use one actor URL per line. Ctrl+S sends.",
        )
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(instructions, layout[2]);
    }

    fn draw_search_overlay(&self, frame: &mut Frame<'_>) {
        let area = centered_rect(70, 40, frame.area());
        let block = Block::default().borders(Borders::ALL).title("Search");
        frame.render_widget(block, area);
        let inner = area.inner(Margin::new(1, 1));
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(inner);
        let query = Paragraph::new(self.search.query.text())
            .block(Block::default().borders(Borders::ALL).title("Query"));
        frame.render_widget(query, layout[0]);
        let hint = Paragraph::new("Enter runs the search. Esc cancels.")
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(hint, layout[1]);
    }

    fn draw_discovery_overlay(&self, frame: &mut Frame<'_>) {
        let area = centered_rect(74, 40, frame.area());
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Discover actor");
        frame.render_widget(block, area);
        let inner = area.inner(Margin::new(1, 1));
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(inner);
        let query = Paragraph::new(self.discovery.query.text()).block(
            Block::default()
                .borders(Borders::ALL)
                .title("@user@example.social or https://..."),
        );
        frame.render_widget(query, layout[0]);
        let hint = Paragraph::new("Enter resolves through the live owner API. Esc cancels.")
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(hint, layout[1]);
    }

    fn entries(&self, tab: Tab) -> Vec<Entry> {
        match tab {
            Tab::Discovery => {
                let details = self.discovered_actor.as_ref().map_or_else(
                    || {
                        "Press g to resolve an ActivityPub actor by @user@host or URL. Press f after resolving to follow.".to_string()
                    },
                    |actor| {
                        format!(
                            "id: {}\nhandle: {}\ninbox: {}\nshared inbox: {}\nstatus: {}\nurl: {}\nicon: {}\n\n{}\n\nPress f to follow this actor.",
                            actor.id,
                            actor.handle.as_deref().unwrap_or(""),
                            actor.inbox,
                            actor.shared_inbox.as_deref().unwrap_or(""),
                            actor.following_status.as_deref().unwrap_or("not-following"),
                            actor.url.as_deref().unwrap_or(""),
                            actor.icon_url.as_deref().unwrap_or(""),
                            actor.summary.as_deref().unwrap_or("")
                        )
                    },
                );
                vec![Entry {
                    title: self
                        .discovered_actor
                        .as_ref()
                        .and_then(|actor| actor.handle.as_deref().or(actor.name.as_deref()))
                        .unwrap_or("No actor resolved")
                        .to_string(),
                    subtitle: self
                        .discovered_actor
                        .as_ref()
                        .map(|actor| actor.id.clone())
                        .unwrap_or_else(|| "Press g to lookup".to_string()),
                    details,
                }]
            }
            Tab::Reader => self
                .reader
                .iter()
                .map(|post| {
                    let author = post
                        .actor_display_name
                        .as_deref()
                        .or(post.actor_username.as_deref())
                        .unwrap_or(&post.actor_id);
                    let cached = self.reader_details.get(&post.object_id);
                    let details = cached.map_or_else(
                        || {
                            format!(
                                "{}\n\nobject: {}\nvisibility: {}\nprotocol: {}\nreply_to: {}\nreplies: {} likes: {} boosts: {}",
                                post.content,
                                post.object_id,
                                post.visibility,
                                post.protocol.as_deref().unwrap_or("activitypub"),
                                post.in_reply_to.as_deref().unwrap_or(""),
                                post.reply_count,
                                post.like_count,
                                post.boost_count
                            )
                        },
                        |detail| {
                            format!(
                                "{}\n\nobject: {}\nvisibility: {:?}\nprotocol: {:?}\nattachments: {}\nreplies: {} likes: {} boosts: {}\n\nPress y to reply, l to like, o to boost.",
                                detail.post.content,
                                detail.post.id,
                                detail.post.visibility,
                                detail.post.protocol,
                                detail.post.attachments.len(),
                                detail.post.reply_count,
                                detail.post.like_count,
                                detail.post.boost_count
                            )
                        },
                    );
                    Entry {
                        title: author.to_string(),
                        subtitle: format!(
                            "{} · {} · replies={} likes={} boosts={}",
                            post.visibility,
                            post.published_at.as_deref().unwrap_or("unknown time"),
                            post.reply_count,
                            post.like_count,
                            post.boost_count
                        ),
                        details,
                    }
                })
                .collect(),
            Tab::Home => self
                .home
                .iter()
                .map(|post| {
                    let display = post
                        .actor_display_name
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .or(post.actor_username.as_deref())
                        .unwrap_or(&post.actor_id);
                    Entry {
                        title: display.to_string(),
                        subtitle: format!(
                            "{} · {} · {}",
                            post.visibility.as_deref().unwrap_or("unknown"),
                            post.published_at.as_deref().unwrap_or("unknown time"),
                            encryption_state(post.encrypted_message.is_some())
                        ),
                        details: timeline_detail(post),
                    }
                })
                .collect(),
            Tab::Posts => self
                .posts
                .iter()
                .map(|post| Entry {
                    title: post
                        .published_at
                        .as_deref()
                        .unwrap_or("unknown time")
                        .to_string(),
                    subtitle: format!(
                        "{} · {}",
                        post.visibility.as_deref().unwrap_or("unknown"),
                        encryption_state(post.encrypted_message.is_some())
                    ),
                    details: post_detail(post),
                })
                .collect(),
            Tab::Friends => self
                .friends
                .iter()
                .map(|friend| Entry {
                    title: friend.friend_actor_id.clone(),
                    subtitle: format!(
                        "follower={} following={} accepted={}",
                        friend.follower_since.as_deref().unwrap_or(""),
                        friend.following_since.as_deref().unwrap_or(""),
                        friend.accepted_at.as_deref().unwrap_or("")
                    ),
                    details: format!(
                        "inbox: {}\nshared inbox: {}\naccepted: {}",
                        friend.friend_inbox.as_deref().unwrap_or(""),
                        friend.friend_shared_inbox.as_deref().unwrap_or(""),
                        friend.accepted_at.as_deref().unwrap_or("")
                    ),
                })
                .collect(),
            Tab::Followers => self
                .followers
                .iter()
                .map(|row| Entry {
                    title: row.follower_actor_id.clone(),
                    subtitle: format!(
                        "status={} created={}",
                        row.status,
                        row.created_at.as_deref().unwrap_or("")
                    ),
                    details: format!(
                        "actor: {}\nstatus: {}\ninbox: {}\nshared inbox: {}\nupdated: {}",
                        row.actor_id,
                        row.status,
                        row.follower_inbox,
                        row.follower_shared_inbox.as_deref().unwrap_or(""),
                        row.updated_at.as_deref().unwrap_or("")
                    ),
                })
                .collect(),
            Tab::Following => self
                .following
                .iter()
                .map(|row| Entry {
                    title: row.target_actor_id.clone(),
                    subtitle: format!(
                        "status={} created={}",
                        row.status,
                        row.created_at.as_deref().unwrap_or("")
                    ),
                    details: format!(
                        "actor: {}\nstatus: {}\ntarget inbox: {}\naccepted: {}",
                        row.actor_id,
                        row.status,
                        row.target_inbox,
                        row.accepted_at.as_deref().unwrap_or("")
                    ),
                })
                .collect(),
            Tab::Notifications => self
                .notifications
                .iter()
                .map(|row| Entry {
                    title: format!(
                        "{}: {}",
                        row.kind,
                        row.actor_username.as_deref().unwrap_or(&row.actor_id)
                    ),
                    subtitle: format!(
                        "{} · {}",
                        if row.read.unwrap_or(false) {
                            "read"
                        } else {
                            "unread"
                        },
                        row.created_at.as_deref().unwrap_or("")
                    ),
                    details: format!(
                        "actor: {}\ncontent: {}\npost: {}\nactivity: {}",
                        row.actor_id,
                        row.content.as_deref().unwrap_or(""),
                        row.post_id.as_deref().unwrap_or(""),
                        row.activity_id.as_deref().unwrap_or("")
                    ),
                })
                .collect(),
            Tab::Profile => self.profile.as_ref().map_or_else(Vec::new, |profile| {
                vec![Entry {
                    title: profile_display_name(profile).to_string(),
                    subtitle: format!(
                        "{} · {}",
                        profile.username,
                        profile.actor_type
                    ),
                    details: profile_detail(profile),
                }]
            }),
            Tab::Deliveries => self
                .deliveries
                .iter()
                .map(|row| Entry {
                    title: row.id.clone(),
                    subtitle: format!(
                        "{} · retry={} · {}",
                        row.status,
                        row.retry_count.unwrap_or(0),
                        row.created_at.as_deref().unwrap_or("")
                    ),
                    details: delivery_detail(row),
                })
                .collect(),
            Tab::DMs => self
                .direct_messages
                .iter()
                .map(|row| Entry {
                    title: row.sender_id.clone(),
                    subtitle: row.published_at.clone(),
                    details: format!(
                        "conversation: {}\ncreated: {}\n\n{}",
                        row.conversation_id,
                        row.created_at.as_deref().unwrap_or(""),
                        row.content
                    ),
                })
                .collect(),
            Tab::Search => {
                let mut entries = Vec::new();
                for post in &self.search_posts {
                    entries.push(Entry {
                        title: format!(
                            "post: {}",
                            post.published_at.as_deref().unwrap_or("unknown time")
                        ),
                        subtitle: format!(
                            "{} · {} · {}",
                            post.visibility.as_deref().unwrap_or("unknown"),
                            post.protocol.as_deref().unwrap_or("activitypub"),
                            encryption_state(post.encrypted_message.is_some())
                        ),
                        details: post_detail(post),
                    });
                }
                for user in &self.search_users {
                    entries.push(Entry {
                        title: format!("user: {}", user.actor_id),
                        subtitle: format!("{} · {}", user.relation, user.status),
                        details: format!(
                            "actor: {}\nrelation: {}\nstatus: {}\ncreated: {}",
                            user.actor_id,
                            user.relation,
                            user.status,
                            user.created_at.as_deref().unwrap_or("")
                        ),
                    });
                }
                entries
            }
            Tab::Bluesky => self
                .bluesky_feed
                .iter()
                .map(|item| {
                    let post = &item.post;
                    let title = post.author.handle.clone();
                    let subtitle = post
                        .record
                        .created_at
                        .as_deref()
                        .unwrap_or("unknown time")
                        .to_string();
                    let details = format!(
                        "{}\n\nuri: {}\ncid: {}\nreplies: {} reposts: {} likes: {}",
                        post.record.text.as_deref().unwrap_or(""),
                        post.uri,
                        post.cid.as_deref().unwrap_or(""),
                        post.reply_count.unwrap_or(0),
                        post.repost_count.unwrap_or(0),
                        post.like_count.unwrap_or(0)
                    );
                    Entry {
                        title,
                        subtitle,
                        details,
                    }
                })
                .collect(),
            Tab::Sources => self
                .source_items
                .iter()
                .map(|item| Entry {
                    title: item.title.clone(),
                    subtitle: format!(
                        "{} · {}{}",
                        item.source_type,
                        item.published_at
                            .as_deref()
                            .or(item.fetched_at.as_deref())
                            .unwrap_or("unknown time"),
                        if item.read.unwrap_or(0) == 1 {
                            " · read"
                        } else {
                            ""
                        }
                    ),
                    details: format!(
                        "source: {}\nurl: {}\nauthor: {}\npolicy: {}\n\n{}",
                        item.source_id,
                        item.canonical_url.as_deref().unwrap_or(""),
                        item.author.as_deref().unwrap_or(""),
                        item.rights_policy_json,
                        item.excerpt.as_deref().unwrap_or("")
                    ),
                })
                .collect(),
            Tab::Stats => self.stats.as_ref().map_or_else(Vec::new, |stats| {
                vec![
                    Entry {
                        title: "followers".to_string(),
                        subtitle: format!(
                            "total={} approved={} pending={} rejected={}",
                            stats.followers_total,
                            stats.followers_approved,
                            stats.followers_pending,
                            stats.followers_rejected
                        ),
                        details: stats_detail(stats),
                    },
                    Entry {
                        title: "following".to_string(),
                        subtitle: stats.following_total.to_string(),
                        details: stats_detail(stats),
                    },
                    Entry {
                        title: "posts".to_string(),
                        subtitle: stats.posts_total.to_string(),
                        details: stats_detail(stats),
                    },
                    Entry {
                        title: "deliveries".to_string(),
                        subtitle: format!(
                            "total={} queued={} failed={}",
                            stats.deliveries_total,
                            stats.deliveries_queued,
                            stats.deliveries_failed
                        ),
                        details: stats_detail(stats),
                    },
                ]
            }),
            Tab::Blocks => self
                .blocks
                .iter()
                .map(|row| Entry {
                    title: row.actor_id.clone(),
                    subtitle: row
                        .blocked_domain
                        .as_deref()
                        .map(|domain| format!("domain={domain}"))
                        .unwrap_or_else(|| "actor block".to_string()),
                    details: format!(
                        "reason: {}\ncreated: {}",
                        row.reason.as_deref().unwrap_or(""),
                        row.created_at.as_deref().unwrap_or("")
                    ),
                })
                .collect(),
        }
    }

    fn empty_detail(&self, tab: Tab) -> String {
        match tab {
            Tab::Stats => "No stats loaded yet".to_string(),
            Tab::Reader => {
                "No live owner timeline rows loaded. Set DAIS_OWNER_TOKEN and refresh.".to_string()
            }
            Tab::Search => "Run a search with /".to_string(),
            Tab::Bluesky => "Login to Bluesky and refresh".to_string(),
            Tab::Profile => {
                "No live owner profile loaded. Set DAIS_OWNER_TOKEN and refresh.".to_string()
            }
            Tab::Sources => "No source items loaded".to_string(),
            _ => "No data".to_string(),
        }
    }
}

async fn load_tab(remote: bool, store: ConfigStore, tab: Tab) -> Result<TabData> {
    match tab {
        Tab::Reader => {
            let client = owner_api_from_env()?;
            let snapshot = client
                .snapshot()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(TabData::Reader(snapshot.home_timeline))
        }
        Tab::Home => {
            let db = D1Client::new(remote)?;
            Ok(TabData::Home(db.home_timeline(50, None).await?))
        }
        Tab::Posts => {
            let db = D1Client::new(remote)?;
            Ok(TabData::Posts(db.list_posts(50).await?))
        }
        Tab::Friends => {
            let db = D1Client::new(remote)?;
            Ok(TabData::Friends(
                db.list_friends("https://social.dais.social/users/social", 50)
                    .await?,
            ))
        }
        Tab::Followers => {
            let client = owner_api_from_env()?;
            let snapshot = client
                .snapshot()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(TabData::Followers(snapshot.followers))
        }
        Tab::Following => {
            let client = owner_api_from_env()?;
            let snapshot = client
                .snapshot()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(TabData::Following(snapshot.following))
        }
        Tab::Notifications => {
            let db = D1Client::new(remote)?;
            Ok(TabData::Notifications(db.list_notifications(50).await?))
        }
        Tab::Profile => {
            let client = owner_api_from_env()?;
            let snapshot = client
                .snapshot()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(TabData::Profile(snapshot.profile))
        }
        Tab::Deliveries => {
            let db = D1Client::new(remote)?;
            Ok(TabData::Deliveries(db.list_deliveries(50, None).await?))
        }
        Tab::DMs => {
            let db = D1Client::new(remote)?;
            Ok(TabData::DMs(db.list_direct_messages(50).await?))
        }
        Tab::Search => Err(anyhow!("search requires a query")),
        Tab::Discovery => Err(anyhow!("discovery requires a target")),
        Tab::Bluesky => {
            let config = store.load_bluesky().context("Bluesky config not found")?;
            let mut client = AtprotoClient::from_config(&config)?;
            client.ensure_session().await?;
            let feed = client.get_timeline(50).await?;
            Ok(TabData::Bluesky(feed.feed))
        }
        Tab::Sources => {
            let db = D1Client::new(remote)?;
            Ok(TabData::Sources(
                db.list_source_items(None, 50, false).await?,
            ))
        }
        Tab::Stats => {
            let db = D1Client::new(remote)?;
            Ok(TabData::Stats(db.stats().await?))
        }
        Tab::Blocks => {
            let db = D1Client::new(remote)?;
            Ok(TabData::Blocks(db.list_blocks(50).await?))
        }
    }
}

fn owner_api_from_env() -> Result<OwnerApiClient> {
    let token = std::env::var("DAIS_OWNER_TOKEN")
        .context("DAIS_OWNER_TOKEN is required for live owner API TUI tabs")?;
    let instance = std::env::var("DAIS_OWNER_INSTANCE_URL")
        .unwrap_or_else(|_| "https://social.dais.social".to_string());
    Ok(OwnerApiClient::new(instance, token))
}

async fn load_search(
    remote: bool,
    _store: ConfigStore,
    query: &str,
) -> Result<(Vec<D1Post>, Vec<D1User>)> {
    let db = D1Client::new(remote)?;
    let posts = db.search_posts(query, 50).await?;
    let users = db.search_users(query, 50).await?;
    Ok((posts, users))
}

fn stats_detail(stats: &ServerStats) -> String {
    format!(
        "followers total: {}\nfollowers approved: {}\nfollowers pending: {}\nfollowers rejected: {}\nfollowing total: {}\nposts total: {}\nposts public: {}\nposts private: {}\nposts direct: {}\nposts encrypted: {}\nposts media: {}\nposts dual protocol: {}\nactivities total: {}\ndeliveries total: {}\ndeliveries queued: {}\ndeliveries retry: {}\ndeliveries delivered: {}\ndeliveries failed: {}\nnotifications unread: {}\nblocks total: {}\nallowlist hosts: {}\nclosed network: {}",
        stats.followers_total,
        stats.followers_approved,
        stats.followers_pending,
        stats.followers_rejected,
        stats.following_total,
        stats.posts_total,
        stats.public_posts,
        stats.private_posts,
        stats.direct_posts,
        stats.encrypted_posts,
        stats.media_posts,
        stats.dual_protocol_posts,
        stats.activities_total,
        stats.deliveries_total,
        stats.deliveries_queued,
        stats.deliveries_retry,
        stats.deliveries_delivered,
        stats.deliveries_failed,
        stats.notifications_unread,
        stats.blocks_total,
        stats.allowlist_hosts,
        stats.closed_network,
    )
}

fn profile_display_name(profile: &OwnerProfile) -> &str {
    profile
        .display_name
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(&profile.username)
}

fn profile_detail(profile: &OwnerProfile) -> String {
    format!(
        "public handle: {}\nactor URL: {}\nusername: {}\nactor type: {}\ndisplay name: {}\nsummary: {}\nicon/avatar URL: {}\nimage/header URL: {}\npublic surfaces: ActivityPub actor JSON, HTML profile, Mastodon account API",
        profile.public_handle,
        profile.actor_url,
        profile.username,
        profile.actor_type,
        profile.display_name.as_deref().unwrap_or(""),
        profile.summary.as_deref().unwrap_or(""),
        profile
            .icon
            .as_deref()
            .or(profile.avatar_url.as_deref())
            .unwrap_or(""),
        profile
            .image
            .as_deref()
            .or(profile.header_url.as_deref())
            .unwrap_or(""),
    )
}

fn timeline_detail(post: &D1TimelinePost) -> String {
    format!(
        "actor: {}\nusername: {}\nvisibility: {}\nprotocol: {}\npublished: {}\nupdated: {}\n{}\n{}",
        post.actor_id,
        post.actor_username.as_deref().unwrap_or(""),
        post.visibility.as_deref().unwrap_or("unknown"),
        post.protocol.as_deref().unwrap_or("activitypub"),
        post.published_at.as_deref().unwrap_or(""),
        post.updated_at.as_deref().unwrap_or(""),
        encryption_state(post.encrypted_message.is_some()),
        post.content
    )
}

fn post_detail(post: &D1Post) -> String {
    format!(
        "id: {}\nvisibility: {}\nprotocol: {}\npublished: {}\natproto: {}\nattachments: {}\n{}\n{}",
        post.id,
        post.visibility.as_deref().unwrap_or("unknown"),
        post.protocol.as_deref().unwrap_or("activitypub"),
        post.published_at.as_deref().unwrap_or(""),
        post.atproto_uri.as_deref().unwrap_or(""),
        post.media_attachments.as_deref().unwrap_or(""),
        encryption_state(post.encrypted_message.is_some()),
        post.content
    )
}

fn delivery_detail(delivery: &D1Delivery) -> String {
    format!(
        "id: {}\npost: {}\ntarget: {}\nprotocol: {}\nstatus: {}\nretry count: {}\ncreated: {}\nlast attempt: {}\ndelivered: {}\nerror: {}",
        delivery.id,
        delivery.post_id,
        delivery.target_url,
        delivery.protocol,
        delivery.status,
        delivery.retry_count.unwrap_or(0),
        delivery.created_at.as_deref().unwrap_or(""),
        delivery.last_attempt_at.as_deref().unwrap_or(""),
        delivery.delivered_at.as_deref().unwrap_or(""),
        delivery.error_message.as_deref().unwrap_or("")
    )
}

fn encryption_state(encrypted: bool) -> &'static str {
    if encrypted {
        "[encrypted]"
    } else {
        "[plaintext]"
    }
}

fn compose_field_title(label: &str, active: bool) -> String {
    if active {
        format!("> {label}")
    } else {
        label.to_string()
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
