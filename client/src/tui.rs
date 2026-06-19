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
use crate::routing::{Protocol, Visibility};
use dais_client_core::{
    ComposeDraft as OwnerComposeDraft, ModerationBlockRow, OwnerApiClient, OwnerDelivery,
    OwnerDirectMessage, OwnerDiscoveredActor, OwnerFollower, OwnerFollowing, OwnerFriend,
    OwnerInteraction, OwnerNotification, OwnerPost, OwnerPostDetail, OwnerProfile,
    OwnerProfileUpdate, OwnerSearchPost, OwnerSearchSourceItem, OwnerSearchUser, OwnerSourceAdd,
    OwnerSources, OwnerStats, OwnerTimelinePost, ProtocolRoute as OwnerProtocolRoute,
    SourceSubscription, Visibility as OwnerVisibility,
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
    SourceAdd,
    Discovery,
    ProfileEdit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ComposeField {
    Body,
    Recipients,
    DirectRecipients,
    ReplyTo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProfileField {
    ActorType,
    DisplayName,
    Summary,
    Icon,
    Image,
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
struct ProfileEditState {
    actor_type: TextBuffer,
    display_name: TextBuffer,
    summary: TextBuffer,
    icon: TextBuffer,
    image: TextBuffer,
    field: ProfileField,
}

impl ProfileEditState {
    fn new() -> Self {
        Self {
            actor_type: TextBuffer::from_text("Person"),
            display_name: TextBuffer::new(),
            summary: TextBuffer::new(),
            icon: TextBuffer::new(),
            image: TextBuffer::new(),
            field: ProfileField::DisplayName,
        }
    }

    fn from_profile(profile: &OwnerProfile) -> Self {
        Self {
            actor_type: TextBuffer::from_text(&profile.actor_type),
            display_name: TextBuffer::from_text(profile.display_name.as_deref().unwrap_or("")),
            summary: TextBuffer::from_text(profile.summary.as_deref().unwrap_or("")),
            icon: TextBuffer::from_text(
                profile
                    .icon
                    .as_deref()
                    .or(profile.avatar_url.as_deref())
                    .unwrap_or(""),
            ),
            image: TextBuffer::from_text(
                profile
                    .image
                    .as_deref()
                    .or(profile.header_url.as_deref())
                    .unwrap_or(""),
            ),
            field: ProfileField::DisplayName,
        }
    }
}

#[derive(Clone, Debug)]
enum TabData {
    Reader(Vec<OwnerTimelinePost>),
    ReaderDetail(OwnerPostDetail),
    Discovery(OwnerDiscoveredActor),
    Home(Vec<OwnerTimelinePost>),
    Posts(Vec<OwnerPost>),
    Friends(Vec<OwnerFriend>),
    Followers(Vec<OwnerFollower>),
    Following(Vec<OwnerFollowing>),
    Notifications(Vec<OwnerNotification>),
    Profile(OwnerProfile),
    Deliveries(Vec<OwnerDelivery>),
    DMs(Vec<OwnerDirectMessage>),
    Search {
        posts: Vec<OwnerSearchPost>,
        users: Vec<OwnerSearchUser>,
        sources: Vec<SourceSubscription>,
        source_items: Vec<OwnerSearchSourceItem>,
        query: String,
    },
    Bluesky(Vec<crate::atproto::FeedItem>),
    Sources(OwnerSources),
    Stats(OwnerStats),
    Blocks(Vec<ModerationBlockRow>),
}

#[derive(Clone, Debug)]
struct Entry {
    title: String,
    subtitle: String,
    details: String,
}

#[derive(Clone)]
enum SourceEntry {
    Subscription(dais_client_core::SourceSubscription),
    Item(dais_client_core::SourceItem),
}

impl SourceEntry {
    fn subscription_id(&self) -> Option<String> {
        match self {
            SourceEntry::Subscription(source) => Some(source.id.clone()),
            SourceEntry::Item(_) => None,
        }
    }

    fn entry(&self) -> Entry {
        match self {
            SourceEntry::Subscription(source) => Entry {
                title: source
                    .title
                    .clone()
                    .unwrap_or_else(|| source.url.clone()),
                subtitle: format!(
                    "subscription · {} · {} · cadence={}m",
                    source.source_type, source.status, source.refresh_cadence_minutes
                ),
                details: format!(
                    "id: {}\nsource type: {}\nurl: {}\nhomepage: {}\nstatus: {}\nlast fetched: {}\nnext fetch: {}\nerrors: {}\nlast error: {}\npolicy: {}",
                    source.id,
                    source.source_type,
                    source.url,
                    source.homepage_url.as_deref().unwrap_or(""),
                    source.status,
                    source.last_fetched_at.as_deref().unwrap_or(""),
                    source.next_fetch_at.as_deref().unwrap_or(""),
                    source.error_count,
                    source.last_error.as_deref().unwrap_or(""),
                    source.policy_json
                ),
            },
            SourceEntry::Item(item) => Entry {
                title: item.title.clone(),
                subtitle: format!(
                    "item · {} · {}",
                    item.source_type,
                    if item.read { "read" } else { "unread" }
                ),
                details: format!(
                    "id: {}\nsource type: {}\nurl: {}\nread: {}\npolicy: {}\n\n{}",
                    item.id,
                    item.source_type,
                    item.canonical_url.as_deref().unwrap_or(""),
                    item.read,
                    item.rights_policy_json,
                    item.excerpt.as_deref().unwrap_or("")
                ),
            },
        }
    }
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
    source_add: SearchState,
    discovery: SearchState,
    profile_edit: ProfileEditState,
    home: Vec<OwnerTimelinePost>,
    posts: Vec<OwnerPost>,
    friends: Vec<OwnerFriend>,
    followers: Vec<OwnerFollower>,
    following: Vec<OwnerFollowing>,
    notifications: Vec<OwnerNotification>,
    profile: Option<OwnerProfile>,
    deliveries: Vec<OwnerDelivery>,
    direct_messages: Vec<OwnerDirectMessage>,
    search_posts: Vec<OwnerSearchPost>,
    search_users: Vec<OwnerSearchUser>,
    search_sources: Vec<SourceSubscription>,
    search_source_items: Vec<OwnerSearchSourceItem>,
    reader: Vec<OwnerTimelinePost>,
    reader_details: HashMap<String, OwnerPostDetail>,
    show_reader_replies: bool,
    discovered_actor: Option<OwnerDiscoveredActor>,
    bluesky_feed: Vec<crate::atproto::FeedItem>,
    sources: OwnerSources,
    stats: Option<OwnerStats>,
    blocks: Vec<ModerationBlockRow>,
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
            source_add: SearchState::new(),
            discovery: SearchState::new(),
            profile_edit: ProfileEditState::new(),
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
            search_sources: Vec::new(),
            search_source_items: Vec::new(),
            reader: Vec::new(),
            reader_details: HashMap::new(),
            show_reader_replies: false,
            discovered_actor: None,
            bluesky_feed: Vec::new(),
            sources: OwnerSources {
                subscriptions: Vec::new(),
                items: Vec::new(),
            },
            stats: None,
            blocks: Vec::new(),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Compose => self.handle_compose_key(key),
            Mode::Search => self.handle_search_key(key),
            Mode::SourceAdd => self.handle_source_add_key(key),
            Mode::Discovery => self.handle_discovery_key(key),
            Mode::ProfileEdit => self.handle_profile_edit_key(key),
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
            KeyCode::Char('p') => self.edit_profile(),
            KeyCode::Char('f') => self.follow_discovered_actor(),
            KeyCode::Char('w') => self.unfollow_selected_following(),
            KeyCode::Char('a') if self.active == Tab::Sources => self.start_source_add(),
            KeyCode::Char('a') => self.approve_selected_follower(),
            KeyCode::Char('x') => self.reject_selected_follower(),
            KeyCode::Char('m') => self.mark_selected_notification_read(),
            KeyCode::Char('u') => self.unblock_selected_block(),
            KeyCode::Char('s') => self.refresh_selected_source(),
            KeyCode::Char('v') => self.remove_selected_source(),
            KeyCode::Char('h') if self.active == Tab::Reader || self.active == Tab::Home => {
                self.show_reader_replies = !self.show_reader_replies;
                self.status = if self.show_reader_replies {
                    "Showing reply posts in reader"
                } else {
                    "Hiding reply posts in reader"
                }
                .to_string();
            }
            KeyCode::Enter => self.load_selected_reader_detail(),
            KeyCode::Char('l') => self.reader_interaction("like"),
            KeyCode::Char('o') => self.reader_interaction("boost"),
            KeyCode::Char('y') => self.reply_to_selected_reader_post(),
            KeyCode::Char('i') => self.show_selected_reader_link(),
            KeyCode::Char('n') => self.open_selected_reader_post(),
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

    fn handle_source_add_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.source_add.query.clear();
                self.status = "Source add canceled".to_string();
            }
            KeyCode::Enter => {
                let input = self.source_add.query.text();
                match parse_source_add_input(&input) {
                    Ok(source) => {
                        self.submit_source_add(source);
                        self.active = Tab::Sources;
                        self.mode = Mode::Normal;
                        self.source_add.query.clear();
                    }
                    Err(error) => {
                        self.status = error.to_string();
                    }
                }
            }
            KeyCode::Backspace => self.source_add.query.backspace(),
            KeyCode::Left => self.source_add.query.move_left(),
            KeyCode::Right => self.source_add.query.move_right(),
            KeyCode::Up => self.source_add.query.move_up(),
            KeyCode::Down => self.source_add.query.move_down(),
            KeyCode::Char(ch) => self.source_add.query.insert_char(ch),
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

    fn handle_profile_edit_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status = "Profile edit canceled".to_string();
            }
            KeyCode::Tab => {
                self.profile_edit.field = match self.profile_edit.field {
                    ProfileField::ActorType => ProfileField::DisplayName,
                    ProfileField::DisplayName => ProfileField::Summary,
                    ProfileField::Summary => ProfileField::Icon,
                    ProfileField::Icon => ProfileField::Image,
                    ProfileField::Image => ProfileField::ActorType,
                };
            }
            KeyCode::BackTab => {
                self.profile_edit.field = match self.profile_edit.field {
                    ProfileField::ActorType => ProfileField::Image,
                    ProfileField::DisplayName => ProfileField::ActorType,
                    ProfileField::Summary => ProfileField::DisplayName,
                    ProfileField::Icon => ProfileField::Summary,
                    ProfileField::Image => ProfileField::Icon,
                };
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.save_profile_edit();
            }
            KeyCode::Enter => {
                if self.profile_edit.field == ProfileField::Summary {
                    self.current_profile_buffer().insert_newline();
                }
            }
            KeyCode::Backspace => {
                self.current_profile_buffer().backspace();
            }
            KeyCode::Left => self.current_profile_buffer().move_left(),
            KeyCode::Right => self.current_profile_buffer().move_right(),
            KeyCode::Up => self.current_profile_buffer().move_up(),
            KeyCode::Down => self.current_profile_buffer().move_down(),
            KeyCode::Char(ch) => self.current_profile_buffer().insert_char(ch),
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

    fn current_profile_buffer(&mut self) -> &mut TextBuffer {
        match self.profile_edit.field {
            ProfileField::ActorType => &mut self.profile_edit.actor_type,
            ProfileField::DisplayName => &mut self.profile_edit.display_name,
            ProfileField::Summary => &mut self.profile_edit.summary,
            ProfileField::Icon => &mut self.profile_edit.icon,
            ProfileField::Image => &mut self.profile_edit.image,
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
                Ok((posts, users, sources, source_items)) => {
                    let _ = tx.send(Message::Loaded(
                        Tab::Search,
                        TabData::Search {
                            posts,
                            users,
                            sources,
                            source_items,
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

    fn edit_profile(&mut self) {
        if self.active != Tab::Profile {
            self.active = Tab::Profile;
        }
        if let Some(profile) = self.profile.as_ref() {
            self.profile_edit = ProfileEditState::from_profile(profile);
            self.mode = Mode::ProfileEdit;
            self.status = "Editing profile".to_string();
        } else {
            self.status = "Load the Profile tab before editing".to_string();
            self.refresh(Tab::Profile);
        }
    }

    fn save_profile_edit(&mut self) {
        let actor_type = self.profile_edit.actor_type.text().trim().to_string();
        if !matches!(actor_type.as_str(), "Person" | "Group" | "Organization") {
            self.status = "Actor type must be Person, Group, or Organization".to_string();
            return;
        }
        let update = OwnerProfileUpdate {
            actor_type: Some(actor_type),
            display_name: optional_trimmed(self.profile_edit.display_name.text()),
            summary: optional_trimmed(self.profile_edit.summary.text()),
            icon: optional_trimmed(self.profile_edit.icon.text()),
            image: optional_trimmed(self.profile_edit.image.text()),
        };
        let tx = self.tx.clone();
        self.status = "Saving profile".to_string();
        self.mode = Mode::Normal;
        self.loading.insert(Tab::Profile);
        tokio::spawn(async move {
            let client = match owner_api_from_env() {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match client.update_profile(&update).await {
                Ok(profile) => {
                    let _ = tx.send(Message::Loaded(Tab::Profile, TabData::Profile(profile)));
                    let _ = tx.send(Message::Status("Saved profile".to_string()));
                    let _ = tx.send(Message::Refresh(Tab::Stats));
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

        if !self.compose.recipients.text().trim().is_empty() {
            self.status =
                "Owner API compose uses actor URL recipients; clear E2EE keys first".to_string();
            return;
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

        let draft = OwnerComposeDraft {
            text,
            visibility: owner_visibility(self.compose.visibility),
            protocol: owner_protocol(self.compose.protocol),
            encrypt: self.compose.encrypt,
            in_reply_to: reply_to,
            audience_list_id: None,
            recipients: to,
            attachments: Vec::new(),
        };

        let tx = self.tx.clone();
        let client = match owner_api_from_env() {
            Ok(client) => client,
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
            match client.create_post(&draft).await {
                Ok(result) => {
                    let mut status = format!("Published owner API post {}", result.id);
                    status.push_str(&format!(
                        "; deliveries queued {}",
                        result.delivery_ids.len()
                    ));
                    let _ = tx.send(Message::Status(status));
                    let _ = tx.send(Message::Refresh(Tab::Home));
                    let _ = tx.send(Message::Refresh(Tab::Posts));
                    if draft.protocol == OwnerProtocolRoute::AtProto
                        || draft.protocol == OwnerProtocolRoute::Both
                    {
                        let _ = tx.send(Message::Refresh(Tab::Bluesky));
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
        tokio::spawn(async move {
            let client = match owner_api_from_env() {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match client.mark_notification_read(&id).await {
                Ok(_) => {
                    let _ = tx.send(Message::Status(format!("Marked notification {id} read")));
                    let _ = tx.send(Message::Refresh(Tab::Notifications));
                    let _ = tx.send(Message::Refresh(Tab::Stats));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn selected_source_subscription_id(&self) -> Option<String> {
        self.source_entries()
            .get(self.selected(Tab::Sources))
            .and_then(SourceEntry::subscription_id)
    }

    fn start_source_add(&mut self) {
        self.active = Tab::Sources;
        self.source_add.query.clear();
        self.mode = Mode::SourceAdd;
        self.status = "Add source".to_string();
    }

    fn source_entries(&self) -> Vec<SourceEntry> {
        self.sources
            .subscriptions
            .iter()
            .cloned()
            .map(SourceEntry::Subscription)
            .chain(self.sources.items.iter().cloned().map(SourceEntry::Item))
            .collect()
    }

    fn submit_source_add(&mut self, source: OwnerSourceAdd) {
        let url = source.url.clone();
        let tx = self.tx.clone();
        self.status = format!("Adding source {url}");
        tokio::spawn(async move {
            let client = match owner_api_from_env() {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match client.add_source(&source).await {
                Ok(result) => {
                    let title = result
                        .source
                        .title
                        .unwrap_or_else(|| result.source.url.clone());
                    let _ = tx.send(Message::Status(format!("Added source {title}")));
                    let _ = tx.send(Message::Refresh(Tab::Sources));
                    let _ = tx.send(Message::Refresh(Tab::Stats));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn refresh_selected_source(&mut self) {
        let Some(id) = self.selected_source_subscription_id() else {
            self.status = "Select a source subscription to refresh".to_string();
            return;
        };
        let tx = self.tx.clone();
        self.status = format!("Refreshing source {id}");
        tokio::spawn(async move {
            let client = match owner_api_from_env() {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match client.refresh_sources(Some(&id)).await {
                Ok(result) => {
                    let status = result
                        .items
                        .iter()
                        .find(|item| item.id == id)
                        .and_then(|item| item.error.as_ref().map(|error| format!("{id}: {error}")))
                        .unwrap_or_else(|| format!("Refreshed source {id}"));
                    let _ = tx.send(Message::Status(status));
                    let _ = tx.send(Message::Refresh(Tab::Sources));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn remove_selected_source(&mut self) {
        let Some(id) = self.selected_source_subscription_id() else {
            self.status = "Select a source subscription to remove".to_string();
            return;
        };
        let tx = self.tx.clone();
        self.status = format!("Removing source {id}");
        tokio::spawn(async move {
            let client = match owner_api_from_env() {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match client.remove_source(&id).await {
                Ok(_) => {
                    let _ = tx.send(Message::Status(format!("Removed source {id}")));
                    let _ = tx.send(Message::Refresh(Tab::Sources));
                    let _ = tx.send(Message::Refresh(Tab::Stats));
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
        let value = row
            .blocked_domain
            .clone()
            .filter(|domain| !domain.is_empty())
            .unwrap_or_else(|| row.actor_id.clone());
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let client = match owner_api_from_env() {
                Ok(client) => client,
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                    return;
                }
            };
            match client.unblock(&value).await {
                Ok(_) => {
                    let _ = tx.send(Message::Status(format!("Unblocked {value}")));
                    let _ = tx.send(Message::Refresh(Tab::Blocks));
                    let _ = tx.send(Message::Refresh(Tab::Stats));
                }
                Err(error) => {
                    let _ = tx.send(Message::Error(error.to_string()));
                }
            }
        });
    }

    fn selected_reader_object_id(&self) -> Option<String> {
        self.visible_reader_posts()
            .get(self.selected(Tab::Reader))
            .map(|row| row.object_id.clone())
    }

    fn visible_reader_posts(&self) -> Vec<&OwnerTimelinePost> {
        self.reader
            .iter()
            .filter(|post| {
                self.show_reader_replies || post.in_reply_to.as_deref().unwrap_or("").is_empty()
            })
            .collect()
    }

    fn visible_home_posts(&self) -> Vec<&OwnerTimelinePost> {
        self.home
            .iter()
            .filter(|post| {
                self.show_reader_replies || post.in_reply_to.as_deref().unwrap_or("").is_empty()
            })
            .collect()
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

    fn show_selected_reader_link(&mut self) {
        if self.active != Tab::Reader {
            return;
        }
        let Some(object_id) = self.selected_reader_object_id() else {
            return;
        };
        self.status = format!("Link: {object_id}");
    }

    fn open_selected_reader_post(&mut self) {
        if self.active != Tab::Reader {
            return;
        }
        let Some(object_id) = self.selected_reader_object_id() else {
            return;
        };
        match open_url(&object_id) {
            Ok(()) => {
                self.status = format!("Opened {object_id}");
            }
            Err(error) => {
                self.status = error.to_string();
            }
        }
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
                        sources,
                        source_items,
                        query,
                    } => {
                        self.search_posts = posts;
                        self.search_users = users;
                        self.search_sources = sources;
                        self.search_source_items = source_items;
                        self.search.query = TextBuffer::from_text(&query);
                    }
                    TabData::Bluesky(value) => self.bluesky_feed = value,
                    TabData::Sources(value) => self.sources = value,
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
        } else if self.mode == Mode::SourceAdd {
            self.draw_source_add_overlay(frame);
        } else if self.mode == Mode::Discovery {
            self.draw_discovery_overlay(frame);
        } else if self.mode == Mode::ProfileEdit {
            self.draw_profile_edit_overlay(frame);
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
            Mode::Normal => "q quit · tab switch · r refresh · h replies · g discover · f follow · w unfollow · enter detail · y/l/o actions · i link · n open",
            Mode::Compose => "ctrl+s send · tab field · v visibility · p protocol · e e2ee · esc cancel",
            Mode::Search => "enter search · esc cancel · backspace edit",
            Mode::SourceAdd => "enter add source/watch · esc cancel · format: rss|atom|api|watch_rss|watch_activitypub_actor|watch_bluesky_actor target [title]",
            Mode::Discovery => "enter lookup actor · esc cancel · backspace edit",
            Mode::ProfileEdit => "ctrl+s save profile · tab field · enter newline in summary · esc cancel",
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
                Constraint::Length(5),
                Constraint::Min(8),
                Constraint::Length(5),
            ])
            .split(inner);

        let meta = Paragraph::new(vec![
            Line::from(format!("Audience: {}", self.compose.visibility)),
            Line::from(audience_description(self.compose.visibility)),
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
                        "Legacy E2EE keys",
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
            "Owner API compose uses actor URLs for direct/E2EE recipients. Leave legacy E2EE keys empty. Ctrl+S sends.",
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

    fn draw_source_add_overlay(&self, frame: &mut Frame<'_>) {
        let area = centered_rect(76, 40, frame.area());
        let block = Block::default().borders(Borders::ALL).title("Add source");
        frame.render_widget(block, area);
        let inner = area.inner(Margin::new(1, 1));
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(inner);
        let query = Paragraph::new(self.source_add.query.text()).block(
            Block::default()
                .borders(Borders::ALL)
                .title("rss|atom|jsonfeed|api url [title]"),
        );
        frame.render_widget(query, layout[0]);
        let hint =
            Paragraph::new("Enter subscribes as private reader-only by default. Esc cancels.")
                .block(Block::default().borders(Borders::ALL));
        frame.render_widget(hint, layout[1]);
    }

    fn draw_profile_edit_overlay(&self, frame: &mut Frame<'_>) {
        let area = centered_rect(82, 84, frame.area());
        let block = Block::default().borders(Borders::ALL).title("Edit profile");
        frame.render_widget(block, area);
        let inner = area.inner(Margin::new(1, 1));
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(6),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
            ])
            .split(inner);

        let actor_type = Paragraph::new(self.profile_edit.actor_type.text()).block(
            Block::default()
                .borders(Borders::ALL)
                .title(profile_field_title(
                    "Actor type",
                    self.profile_edit.field == ProfileField::ActorType,
                )),
        );
        frame.render_widget(actor_type, layout[0]);

        let display_name = Paragraph::new(self.profile_edit.display_name.text()).block(
            Block::default()
                .borders(Borders::ALL)
                .title(profile_field_title(
                    "Display name",
                    self.profile_edit.field == ProfileField::DisplayName,
                )),
        );
        frame.render_widget(display_name, layout[1]);

        let summary = Paragraph::new(self.profile_edit.summary.text())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(profile_field_title(
                        "Summary",
                        self.profile_edit.field == ProfileField::Summary,
                    )),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(summary, layout[2]);

        let icon = Paragraph::new(self.profile_edit.icon.text()).block(
            Block::default()
                .borders(Borders::ALL)
                .title(profile_field_title(
                    "Icon/avatar URL",
                    self.profile_edit.field == ProfileField::Icon,
                )),
        );
        frame.render_widget(icon, layout[3]);

        let image = Paragraph::new(self.profile_edit.image.text()).block(
            Block::default()
                .borders(Borders::ALL)
                .title(profile_field_title(
                    "Image/header URL",
                    self.profile_edit.field == ProfileField::Image,
                )),
        );
        frame.render_widget(image, layout[4]);

        let hint = Paragraph::new("Actor type: Person, Group, or Organization. Empty optional fields are left unchanged by the owner API.")
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(hint, layout[5]);
    }

    fn entries(&self, tab: Tab) -> Vec<Entry> {
        match tab {
            Tab::Discovery => {
                let details = self.discovered_actor.as_ref().map_or_else(
                    || {
                        "Press g to resolve an ActivityPub actor by @user@host or URL. Press f after resolving to follow.".to_string()
                    },
                    |actor| {
                        let target_post = actor.target_public_post.as_ref().map_or_else(
                            || "target public post: none".to_string(),
                            |post| {
                                format!(
                                    "target public post: {}\ntype: {}\nauthor: {}\npublished: {}\n{}",
                                    post.url.as_deref().unwrap_or(&post.id),
                                    post.kind,
                                    post.actor_id.as_deref().unwrap_or(""),
                                    post.published.as_deref().unwrap_or("undated"),
                                    post.content
                                )
                            },
                        );
                        let recent = if actor.recent_public_posts.is_empty() {
                            "recent public posts: none returned".to_string()
                        } else {
                            actor
                                .recent_public_posts
                                .iter()
                                .map(|post| {
                                    format!(
                                        "- {} · {}",
                                        post.published.as_deref().unwrap_or("undated"),
                                        post.name
                                            .as_deref()
                                            .filter(|value| !value.is_empty())
                                            .unwrap_or(&post.content)
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                        };
                        format!(
                            "id: {}\ntype: {}\nhandle: {}\ninbox: {}\nshared inbox: {}\nstatus: {}\nurl: {}\nicon: {}\n\n{}\n\n{}\n\nRecent public posts\n{}\n\nPress f to follow this actor.",
                            actor.id,
                            actor.actor_type.as_deref().unwrap_or(""),
                            actor.handle.as_deref().unwrap_or(""),
                            actor.inbox,
                            actor.shared_inbox.as_deref().unwrap_or(""),
                            actor.following_status.as_deref().unwrap_or("not-following"),
                            actor.url.as_deref().unwrap_or(""),
                            actor.icon_url.as_deref().unwrap_or(""),
                            actor.summary.as_deref().unwrap_or(""),
                            target_post,
                            recent
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
                .visible_reader_posts()
                .into_iter()
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
                            audience_label(&post.visibility),
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
                .visible_home_posts()
                .into_iter()
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
                            "{} · {} · replies={} likes={} boosts={}",
                            audience_label(&post.visibility),
                            post.published_at.as_deref().unwrap_or("unknown time"),
                            post.reply_count,
                            post.like_count,
                            post.boost_count
                        ),
                        details: owner_timeline_detail(post),
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
                        "{:?} · {:?} · replies={} likes={} boosts={}",
                        audience_label(&format!("{:?}", post.visibility)), post.protocol, post.reply_count, post.like_count, post.boost_count
                    ),
                    details: owner_post_detail(post),
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
                        if owner_notification_read(row) {
                            "read"
                        } else {
                            "unread"
                        },
                        row.created_at.as_deref().unwrap_or("")
                    ),
                    details: format!(
                        "actor: {}\ncontent: {}\ncontext: {}\npost: {}\nactivity: {}",
                        row.actor_id,
                        row.content.as_deref().unwrap_or(""),
                        row.context_post_content.as_deref().unwrap_or(""),
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
                for source in &self.search_sources {
                    entries.push(Entry {
                        title: format!(
                            "source: {}",
                            source.title.as_deref().unwrap_or(&source.url)
                        ),
                        subtitle: format!("{} · {}", source.source_type, source.status),
                        details: format!(
                            "id: {}\nurl: {}\nhomepage: {}\nstatus: {}\nrefresh: {} min\nlast fetched: {}\nnext fetch: {}\nlast error: {}",
                            source.id,
                            source.url,
                            source.homepage_url.as_deref().unwrap_or(""),
                            source.status,
                            source.refresh_cadence_minutes,
                            source.last_fetched_at.as_deref().unwrap_or(""),
                            source.next_fetch_at.as_deref().unwrap_or(""),
                            source.last_error.as_deref().unwrap_or("")
                        ),
                    });
                }
                for item in &self.search_source_items {
                    entries.push(Entry {
                        title: format!("source item: {}", item.title),
                        subtitle: format!(
                            "{} · {} · {}",
                            item.source_type,
                            if source_item_read(&item.read) {
                                "read"
                            } else {
                                "unread"
                            },
                            item.published_at.as_deref().unwrap_or("unknown time")
                        ),
                        details: format!(
                            "id: {}\nsource: {}\nurl: {}\ncreated: {}\n\n{}",
                            item.id,
                            item.source_id,
                            item.canonical_url.as_deref().unwrap_or(""),
                            item.created_at.as_deref().unwrap_or(""),
                            item.excerpt.as_deref().unwrap_or("")
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
                .source_entries()
                .iter()
                .map(SourceEntry::entry)
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
                    title: row
                        .blocked_domain
                        .as_deref()
                        .filter(|domain| !domain.is_empty())
                        .unwrap_or(&row.actor_id)
                        .to_string(),
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

async fn load_tab(_remote: bool, store: ConfigStore, tab: Tab) -> Result<TabData> {
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
            let client = owner_api_from_env()?;
            let snapshot = client
                .snapshot()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(TabData::Home(snapshot.home_timeline))
        }
        Tab::Posts => {
            let client = owner_api_from_env()?;
            let snapshot = client
                .snapshot()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(TabData::Posts(snapshot.posts))
        }
        Tab::Friends => {
            let client = owner_api_from_env()?;
            let friends = client
                .friends()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(TabData::Friends(friends))
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
            let client = owner_api_from_env()?;
            let notifications = client
                .notifications()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(TabData::Notifications(notifications))
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
            let client = owner_api_from_env()?;
            let deliveries = client
                .deliveries()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(TabData::Deliveries(deliveries))
        }
        Tab::DMs => {
            let client = owner_api_from_env()?;
            let direct_messages = client
                .direct_messages()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(TabData::DMs(direct_messages))
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
            let client = owner_api_from_env()?;
            let sources = client
                .sources()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(TabData::Sources(sources))
        }
        Tab::Stats => {
            let client = owner_api_from_env()?;
            let stats = client
                .stats()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(TabData::Stats(stats))
        }
        Tab::Blocks => {
            let client = owner_api_from_env()?;
            let moderation = client
                .moderation()
                .await
                .map_err(|error| anyhow!(error.to_string()))?;
            Ok(TabData::Blocks(moderation.blocks))
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

fn owner_notification_read(notification: &OwnerNotification) -> bool {
    notification.read == serde_json::Value::Bool(true)
        || notification.read == serde_json::Value::Number(1.into())
        || notification.read == serde_json::Value::String("1".to_string())
        || notification.read == serde_json::Value::String("true".to_string())
}

fn owner_visibility(value: Visibility) -> OwnerVisibility {
    match value {
        Visibility::Public => OwnerVisibility::Public,
        Visibility::Unlisted => OwnerVisibility::Unlisted,
        Visibility::Followers => OwnerVisibility::Followers,
        Visibility::Direct => OwnerVisibility::Direct,
    }
}

fn audience_description(value: Visibility) -> &'static str {
    match value {
        Visibility::Public => "Public: internet-visible",
        Visibility::Unlisted => "Unlisted: link-visible",
        Visibility::Followers => "Followers: approved followers only",
        Visibility::Direct => "Direct: named recipients only",
    }
}

fn audience_label(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "public" => "Public - internet visible".to_string(),
        "unlisted" => "Unlisted - link visible".to_string(),
        "followers" | "private" => "Followers - approved followers".to_string(),
        "direct" => "Direct - named recipients".to_string(),
        _ => format!("{value} - check audience"),
    }
}

fn owner_protocol(value: Protocol) -> OwnerProtocolRoute {
    match value {
        Protocol::ActivityPub => OwnerProtocolRoute::ActivityPub,
        Protocol::Atproto => OwnerProtocolRoute::AtProto,
        Protocol::Both => OwnerProtocolRoute::Both,
    }
}

async fn load_search(
    _remote: bool,
    _store: ConfigStore,
    query: &str,
) -> Result<(
    Vec<OwnerSearchPost>,
    Vec<OwnerSearchUser>,
    Vec<SourceSubscription>,
    Vec<OwnerSearchSourceItem>,
)> {
    let client = owner_api_from_env()?;
    let results = client
        .search(query)
        .await
        .map_err(|error| anyhow!(error.to_string()))?;
    Ok((
        results.posts,
        results.users,
        results.sources,
        results.source_items,
    ))
}

fn stats_detail(stats: &OwnerStats) -> String {
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

fn owner_timeline_detail(post: &OwnerTimelinePost) -> String {
    format!(
        "id: {}\nobject: {}\nactor: {}\nusername: {}\ndisplay name: {}\naudience: {}\nvisibility: {}\nprotocol: {}\nreply to: {}\npublished: {}\nreplies: {}\nlikes: {}\nboosts: {}\n\n{}",
        post.id,
        post.object_id,
        post.actor_id,
        post.actor_username.as_deref().unwrap_or(""),
        post.actor_display_name.as_deref().unwrap_or(""),
        audience_label(&post.visibility),
        post.visibility,
        post.protocol.as_deref().unwrap_or("activitypub"),
        post.in_reply_to.as_deref().unwrap_or(""),
        post.published_at.as_deref().unwrap_or(""),
        post.reply_count,
        post.like_count,
        post.boost_count,
        post.content
    )
}

fn owner_post_detail(post: &OwnerPost) -> String {
    format!(
        "id: {}\ntitle: {}\naudience: {}\nvisibility: {:?}\nprotocol: {:?}\npublished: {}\nencrypted: {}\nattachments: {}\nreplies: {}\nlikes: {}\nboosts: {}\n\n{}",
        post.id,
        post.title.as_deref().unwrap_or(""),
        audience_label(&format!("{:?}", post.visibility)),
        post.visibility,
        post.protocol,
        post.published_at.as_deref().unwrap_or(""),
        post.encrypted,
        post.attachments.len(),
        post.reply_count,
        post.like_count,
        post.boost_count,
        post.content
    )
}

fn post_detail(post: &OwnerSearchPost) -> String {
    format!(
        "id: {}\naudience: {}\nvisibility: {}\nprotocol: {}\npublished: {}\natproto: {}\nattachments: {}\n{}\n{}",
        post.id,
        audience_label(post.visibility.as_deref().unwrap_or("unknown")),
        post.visibility.as_deref().unwrap_or("unknown"),
        post.protocol.as_deref().unwrap_or("activitypub"),
        post.published_at.as_deref().unwrap_or(""),
        post.atproto_uri.as_deref().unwrap_or(""),
        post.media_attachments.as_deref().unwrap_or(""),
        encryption_state(post.encrypted_message.is_some()),
        post.content
    )
}

fn delivery_detail(delivery: &OwnerDelivery) -> String {
    format!(
        "id: {}\npost: {}\ntarget type: {}\ntarget: {}\nprotocol: {}\nactivity: {}\nstatus: {}\nretry count: {}\ncreated: {}\nlast attempt: {}\ndelivered: {}\nerror: {}",
        delivery.id,
        delivery.post_id,
        delivery.target_type.as_deref().unwrap_or(""),
        delivery.target_url,
        delivery.protocol,
        delivery.activity_type.as_deref().unwrap_or(""),
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

fn source_item_read(value: &serde_json::Value) -> bool {
    value == &serde_json::Value::Bool(true)
        || value == &serde_json::Value::Number(1.into())
        || value == &serde_json::Value::String("1".to_string())
        || value == &serde_json::Value::String("true".to_string())
}

fn compose_field_title(label: &str, active: bool) -> String {
    if active {
        format!("> {label}")
    } else {
        label.to_string()
    }
}

fn profile_field_title(label: &str, active: bool) -> String {
    if active {
        format!("> {label}")
    } else {
        label.to_string()
    }
}

fn optional_trimmed(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn parse_source_add_input(input: &str) -> Result<OwnerSourceAdd> {
    let mut parts = input.split_whitespace();
    let source_type = parts
        .next()
        .ok_or_else(|| anyhow!("Use: rss|atom|api|watch_rss|watch_activitypub_actor|watch_bluesky_actor target [title]"))?;
    let url = parts
        .next()
        .ok_or_else(|| anyhow!("Source URL is required"))?;
    let title = parts.collect::<Vec<_>>().join(" ");

    Ok(OwnerSourceAdd {
        source_type: source_type.to_string(),
        url: url.to_string(),
        title: optional_trimmed(title),
        cadence_minutes: Some(60),
        api_secret_name: None,
        private_reader_only: true,
        excerpt_only: true,
        link_required: true,
        attribution_required: true,
        image_allowed: false,
        full_text_allowed: false,
    })
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
    return Err(anyhow!("opening URLs is not supported on this platform"));

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("failed to open {url}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_source_add_input_accepts_type_url_and_title() {
        let source =
            parse_source_add_input("rss https://example.com/feed.xml Example Feed").unwrap();

        assert_eq!(source.source_type, "rss");
        assert_eq!(source.url, "https://example.com/feed.xml");
        assert_eq!(source.title.as_deref(), Some("Example Feed"));
        assert_eq!(source.cadence_minutes, Some(60));
        assert!(source.private_reader_only);
        assert!(source.excerpt_only);
        assert!(source.link_required);
        assert!(source.attribution_required);
        assert!(!source.image_allowed);
        assert!(!source.full_text_allowed);
    }

    #[test]
    fn parse_source_add_input_allows_missing_title() {
        let source = parse_source_add_input("atom https://example.com/atom.xml").unwrap();

        assert_eq!(source.source_type, "atom");
        assert_eq!(source.url, "https://example.com/atom.xml");
        assert_eq!(source.title, None);
    }

    #[test]
    fn parse_source_add_input_requires_url() {
        let error = parse_source_add_input("rss").unwrap_err();

        assert!(error.to_string().contains("Source URL is required"));
    }
}
