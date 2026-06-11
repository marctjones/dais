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
    D1Block, D1Client, D1DirectMessage, D1FollowerRow, D1FollowingRow, D1Friend, D1Notification,
    D1Post, D1TimelinePost, D1User, ServerStats,
};
use crate::posting::{publish_post, PostDraft, PostOutcome};
use crate::routing::{Protocol, Visibility};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
enum Tab {
    Home,
    Posts,
    Friends,
    Followers,
    Following,
    Notifications,
    DMs,
    Search,
    Bluesky,
    Stats,
    Blocks,
}

impl Tab {
    fn all() -> [Tab; 11] {
        [
            Tab::Home,
            Tab::Posts,
            Tab::Friends,
            Tab::Followers,
            Tab::Following,
            Tab::Notifications,
            Tab::DMs,
            Tab::Search,
            Tab::Bluesky,
            Tab::Stats,
            Tab::Blocks,
        ]
    }

    fn title(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Posts => "Posts",
            Tab::Friends => "Friends",
            Tab::Followers => "Followers",
            Tab::Following => "Following",
            Tab::Notifications => "Notifications",
            Tab::DMs => "DMs",
            Tab::Search => "Search",
            Tab::Bluesky => "Bluesky",
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ComposeField {
    Body,
    Recipients,
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
            visibility: Visibility::Followers,
            protocol: Protocol::ActivityPub,
            encrypt: false,
            field: ComposeField::Body,
        }
    }

    fn reset(&mut self) {
        self.body.clear();
        self.recipients.clear();
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
    Home(Vec<D1TimelinePost>),
    Posts(Vec<D1Post>),
    Friends(Vec<D1Friend>),
    Followers(Vec<D1FollowerRow>),
    Following(Vec<D1FollowingRow>),
    Notifications(Vec<D1Notification>),
    DMs(Vec<D1DirectMessage>),
    Search {
        posts: Vec<D1Post>,
        users: Vec<D1User>,
        query: String,
    },
    Bluesky(Vec<crate::atproto::FeedItem>),
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
    home: Vec<D1TimelinePost>,
    posts: Vec<D1Post>,
    friends: Vec<crate::d1::D1Friend>,
    followers: Vec<D1FollowerRow>,
    following: Vec<D1FollowingRow>,
    notifications: Vec<D1Notification>,
    direct_messages: Vec<D1DirectMessage>,
    search_posts: Vec<D1Post>,
    search_users: Vec<D1User>,
    bluesky_feed: Vec<crate::atproto::FeedItem>,
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
            home: Vec::new(),
            posts: Vec::new(),
            friends: Vec::new(),
            followers: Vec::new(),
            following: Vec::new(),
            notifications: Vec::new(),
            direct_messages: Vec::new(),
            search_posts: Vec::new(),
            search_users: Vec::new(),
            bluesky_feed: Vec::new(),
            stats: None,
            blocks: Vec::new(),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Compose => self.handle_compose_key(key),
            Mode::Search => self.handle_search_key(key),
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
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.status = "Search".to_string();
            }
            KeyCode::Char('a') => self.approve_selected_follower(),
            KeyCode::Char('x') => self.reject_selected_follower(),
            KeyCode::Char('m') => self.mark_selected_notification_read(),
            KeyCode::Char('u') => self.unblock_selected_block(),
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
                    ComposeField::Recipients => ComposeField::Body,
                };
            }
            KeyCode::BackTab => {
                self.compose.field = match self.compose.field {
                    ComposeField::Body => ComposeField::Recipients,
                    ComposeField::Recipients => ComposeField::Body,
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
                if self.compose.field == ComposeField::Recipients {
                    self.compose.recipients.insert_newline();
                } else {
                    self.compose.body.insert_newline();
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

    fn current_compose_buffer(&mut self) -> &mut TextBuffer {
        match self.compose.field {
            ComposeField::Body => &mut self.compose.body,
            ComposeField::Recipients => &mut self.compose.recipients,
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
            current.saturating_sub(delta.unsigned_abs() as usize)
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
        if tab == Tab::Search {
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

        let draft = PostDraft {
            text,
            visibility: self.compose.visibility,
            protocol: self.compose.protocol,
            encrypt: self.compose.encrypt,
            recipients: recipients.into_iter().collect(),
            reply_to: None,
            to: Vec::new(),
            e2ee_fallback: crate::cli::E2eeFallbackMode::Strict,
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
        let actor_id = row.actor_id.clone();
        let follower = row.follower_actor_id.clone();
        let tx = self.tx.clone();
        let remote = self.remote;
        self.status = format!("Approving {follower}");
        tokio::spawn(async move {
            let db = match D1Client::new(remote) {
                Ok(db) => db,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match db.approve_follower(&actor_id, &follower).await {
                Ok(()) => {
                    let _ = tx.send(Message::Status(format!("Approved {follower}")));
                    let _ = tx.send(Message::Refresh(Tab::Followers));
                    let _ = tx.send(Message::Refresh(Tab::Friends));
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
        let actor_id = row.actor_id.clone();
        let follower = row.follower_actor_id.clone();
        let tx = self.tx.clone();
        let remote = self.remote;
        self.status = format!("Rejecting {follower}");
        tokio::spawn(async move {
            let db = match D1Client::new(remote) {
                Ok(db) => db,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match db.reject_follower(&actor_id, &follower).await {
                Ok(()) => {
                    let _ = tx.send(Message::Status(format!("Rejected {follower}")));
                    let _ = tx.send(Message::Refresh(Tab::Followers));
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
            match db.unblock_actor(&actor_id).await {
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

    fn selected(&self, tab: Tab) -> usize {
        *self.selection.get(&tab).unwrap_or(&0)
    }

    fn apply(&mut self, message: Message) {
        match message {
            Message::Loaded(tab, data) => {
                self.loading.remove(&tab);
                match data {
                    TabData::Home(value) => self.home = value,
                    TabData::Posts(value) => self.posts = value,
                    TabData::Friends(value) => self.friends = value,
                    TabData::Followers(value) => self.followers = value,
                    TabData::Following(value) => self.following = value,
                    TabData::Notifications(value) => self.notifications = value,
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
            Mode::Normal => "q quit · tab switch · r refresh · c compose · / search · a approve · x reject · m read · u unblock",
            Mode::Compose => "ctrl+s send · tab field · v visibility · p protocol · e e2ee · esc cancel",
            Mode::Search => "enter search · esc cancel · backspace edit",
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
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(layout[1]);
        let body = Paragraph::new(self.compose.body.text())
            .block(Block::default().borders(Borders::ALL).title("Body"))
            .wrap(Wrap { trim: false });
        let recipients = Paragraph::new(self.compose.recipients.text())
            .block(Block::default().borders(Borders::ALL).title("Recipients"))
            .wrap(Wrap { trim: false });
        frame.render_widget(body, editor_chunks[0]);
        frame.render_widget(recipients, editor_chunks[1]);

        let instructions = Paragraph::new(
            "Enter inserts newline. Tab switches field. V toggles audience. P toggles protocol. E toggles E2EE. Ctrl+S sends.",
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

    fn entries(&self, tab: Tab) -> Vec<Entry> {
        match tab {
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
                            "{} · {}",
                            post.visibility.as_deref().unwrap_or("unknown"),
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
                            "total={} failed={}",
                            stats.deliveries_total, stats.deliveries_failed
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
            Tab::Search => "Run a search with /".to_string(),
            Tab::Bluesky => "Login to Bluesky and refresh".to_string(),
            _ => "No data".to_string(),
        }
    }
}

async fn load_tab(remote: bool, store: ConfigStore, tab: Tab) -> Result<TabData> {
    match tab {
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
            let db = D1Client::new(remote)?;
            Ok(TabData::Followers(db.list_followers(50).await?))
        }
        Tab::Following => {
            let db = D1Client::new(remote)?;
            Ok(TabData::Following(db.list_following(50).await?))
        }
        Tab::Notifications => {
            let db = D1Client::new(remote)?;
            Ok(TabData::Notifications(db.list_notifications(50).await?))
        }
        Tab::DMs => {
            let db = D1Client::new(remote)?;
            Ok(TabData::DMs(db.list_direct_messages(50).await?))
        }
        Tab::Search => Err(anyhow!("search requires a query")),
        Tab::Bluesky => {
            let config = store.load_bluesky().context("Bluesky config not found")?;
            let mut client = AtprotoClient::from_config(&config)?;
            client.ensure_session().await?;
            let feed = client.get_timeline(50).await?;
            Ok(TabData::Bluesky(feed.feed))
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
        "followers total: {}\nfollowers approved: {}\nfollowers pending: {}\nfollowers rejected: {}\nfollowing total: {}\nposts total: {}\nposts dual protocol: {}\nactivities total: {}\ndeliveries total: {}\ndeliveries failed: {}",
        stats.followers_total,
        stats.followers_approved,
        stats.followers_pending,
        stats.followers_rejected,
        stats.following_total,
        stats.posts_total,
        stats.dual_protocol_posts,
        stats.activities_total,
        stats.deliveries_total,
        stats.deliveries_failed,
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
        "id: {}\nvisibility: {}\nprotocol: {}\npublished: {}\natproto: {}\n{}\n{}",
        post.id,
        post.visibility.as_deref().unwrap_or("unknown"),
        post.protocol.as_deref().unwrap_or("activitypub"),
        post.published_at.as_deref().unwrap_or(""),
        post.atproto_uri.as_deref().unwrap_or(""),
        encryption_state(post.encrypted_message.is_some()),
        post.content
    )
}

fn encryption_state(encrypted: bool) -> &'static str {
    if encrypted {
        "[encrypted]"
    } else {
        "[plaintext]"
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
