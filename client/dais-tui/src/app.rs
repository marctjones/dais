//! TUI application state and event handling.
//!
//! Immediate-mode (CLIENT_REDESIGN.md §2): a single [`App`] is the source of truth;
//! the render fn is a pure `&App -> frame`; the event loop mutates `App`. Keypresses
//! never block on the network (reads come from the local store).

use dais_client::model::{Feed, FollowRequest, Post, Visibility};
use dais_client::Client;

/// The switchable views — "folders" in the mail-client model (§5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Home,
    Mentions,
    Requests,
    Dms,
    Sent,
    Notifs,
}

impl View {
    pub fn title(self) -> &'static str {
        match self {
            View::Home => "Home",
            View::Mentions => "Mentions",
            View::Requests => "Requests",
            View::Dms => "DMs",
            View::Sent => "Sent",
            View::Notifs => "Notifs",
        }
    }

    pub fn all() -> [View; 6] {
        [
            View::Home,
            View::Mentions,
            View::Requests,
            View::Dms,
            View::Sent,
            View::Notifs,
        ]
    }

    /// The store feed backing list views (None for Requests/DMs/Notifs).
    pub fn feed(self) -> Option<Feed> {
        match self {
            View::Home => Some(Feed::Home),
            View::Mentions => Some(Feed::Mentions),
            View::Sent => Some(Feed::Sent),
            _ => None,
        }
    }
}

/// What the UI is currently focused on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Composer,
    Palette,
    Thread,
}

/// Privacy-forward composer state (§5.3).
pub struct Composer {
    pub text: String,
    pub visibility: Visibility,
    pub encrypt: bool,
    pub reply_to: Option<String>,
    pub reply_handle: Option<String>,
}

impl Composer {
    fn new(default_vis: Visibility) -> Self {
        Composer {
            text: String::new(),
            visibility: default_vis,
            encrypt: false,
            reply_to: None,
            reply_handle: None,
        }
    }

    /// Characters remaining (mirrors the 500-char mockup budget).
    pub fn remaining(&self) -> i64 {
        500 - self.text.chars().count() as i64
    }
}

/// The command palette (§5.5) — discoverability backstop.
pub struct Palette {
    pub query: String,
    pub selected: usize,
}

/// One palette action.
#[derive(Clone)]
pub struct PaletteItem {
    pub label: &'static str,
    pub hint: &'static str,
    pub action: Action,
}

#[derive(Clone, Copy)]
pub enum Action {
    Compose,
    GoHome,
    GoMentions,
    GoRequests,
    GoDms,
    GoSent,
    GoNotifs,
    ToggleHelp,
    Refresh,
}

pub struct App {
    pub client: Client,
    pub view: View,
    pub mode: Mode,
    pub posts: Vec<Post>,
    pub requests: Vec<FollowRequest>,
    pub selected: usize,
    pub composer: Composer,
    pub palette: Palette,
    pub thread_replies: Vec<Post>,
    pub leader: bool,
    pub show_help: bool,
    pub status: String,
    pub should_quit: bool,
}

impl App {
    pub fn new(client: Client) -> Self {
        let default_vis = client.config.default_visibility();
        let mut app = App {
            client,
            view: View::Home,
            mode: Mode::Normal,
            posts: Vec::new(),
            requests: Vec::new(),
            selected: 0,
            composer: Composer::new(default_vis),
            palette: Palette {
                query: String::new(),
                selected: 0,
            },
            thread_replies: Vec::new(),
            leader: false,
            show_help: false,
            status: "Welcome to dais — press ? for help".to_string(),
            should_quit: false,
        };
        app.reload();
        app
    }

    /// Reload the current view from the local store.
    pub fn reload(&mut self) {
        match self.view {
            View::Requests => {
                self.requests = self.client.requests().unwrap_or_default();
                if self.selected >= self.requests.len() {
                    self.selected = self.requests.len().saturating_sub(1);
                }
            }
            _ => {
                let feed = self.view.feed().unwrap_or(Feed::Home);
                self.posts = self.client.timeline(feed, 200).unwrap_or_default();
                if self.selected >= self.posts.len() {
                    self.selected = self.posts.len().saturating_sub(1);
                }
            }
        }
    }

    pub fn unread(&self, view: View) -> u32 {
        match view.feed() {
            Some(feed) => self.client.store.unread_count(feed).unwrap_or(0),
            None if view == View::Requests => self
                .client
                .requests()
                .map(|r| r.iter().filter(|x| x.unread).count() as u32)
                .unwrap_or(0),
            _ => 0,
        }
    }

    pub fn selected_post(&self) -> Option<&Post> {
        self.posts.get(self.selected)
    }

    pub fn selected_request(&self) -> Option<&FollowRequest> {
        self.requests.get(self.selected)
    }

    pub fn current_len(&self) -> usize {
        match self.view {
            View::Requests => self.requests.len(),
            _ => self.posts.len(),
        }
    }

    fn switch(&mut self, view: View) {
        self.view = view;
        self.selected = 0;
        self.mode = Mode::Normal;
        self.reload();
        self.status = format!("→ {}", view.title());
    }

    fn move_by(&mut self, delta: i64) {
        let len = self.current_len();
        if len == 0 {
            return;
        }
        let cur = self.selected as i64;
        let next = (cur + delta).clamp(0, len as i64 - 1);
        self.selected = next as usize;
    }

    // ---- palette ---------------------------------------------------------

    pub fn palette_items(&self) -> Vec<PaletteItem> {
        let all = vec![
            PaletteItem { label: "Compose post", hint: "c", action: Action::Compose },
            PaletteItem { label: "Go: Home", hint: "g h", action: Action::GoHome },
            PaletteItem { label: "Go: Mentions", hint: "g m", action: Action::GoMentions },
            PaletteItem { label: "Go: Requests", hint: "g r", action: Action::GoRequests },
            PaletteItem { label: "Go: DMs", hint: "g d", action: Action::GoDms },
            PaletteItem { label: "Go: Sent", hint: "g s", action: Action::GoSent },
            PaletteItem { label: "Go: Notifications", hint: "g n", action: Action::GoNotifs },
            PaletteItem { label: "Refresh current view", hint: "", action: Action::Refresh },
            PaletteItem { label: "Toggle help", hint: "?", action: Action::ToggleHelp },
        ];
        let q = self.palette.query.to_lowercase();
        if q.is_empty() {
            all
        } else {
            all.into_iter()
                .filter(|i| i.label.to_lowercase().contains(&q))
                .collect()
        }
    }

    fn run_action(&mut self, action: Action) {
        self.mode = Mode::Normal;
        self.palette.query.clear();
        self.palette.selected = 0;
        match action {
            Action::Compose => self.open_composer(false),
            Action::GoHome => self.switch(View::Home),
            Action::GoMentions => self.switch(View::Mentions),
            Action::GoRequests => self.switch(View::Requests),
            Action::GoDms => self.switch(View::Dms),
            Action::GoSent => self.switch(View::Sent),
            Action::GoNotifs => self.switch(View::Notifs),
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::Refresh => {
                self.reload();
                self.status = "Refreshed".into();
            }
        }
    }

    // ---- composer --------------------------------------------------------

    fn open_composer(&mut self, reply: bool) {
        self.composer = Composer {
            text: String::new(),
            visibility: self.client.config.default_visibility(),
            encrypt: self.client.config.defaults.encrypt,
            reply_to: None,
            reply_handle: None,
        };
        if reply {
            if let Some(p) = self.selected_post() {
                let (id, handle) = (p.id.clone(), p.author_handle.clone());
                self.composer.reply_to = Some(id);
                self.composer.reply_handle = Some(handle);
            }
        }
        self.mode = Mode::Composer;
    }

    fn send_composer(&mut self) {
        let text = self.composer.text.trim().to_string();
        if text.is_empty() {
            self.status = "Nothing to send".into();
            self.mode = Mode::Normal;
            return;
        }
        let res = self.client.compose(
            &text,
            self.composer.visibility,
            self.composer.encrypt,
            self.composer.reply_to.as_deref(),
        );
        match res {
            Ok(r) => {
                self.status = format!(
                    "Staged draft #{} ({}{})",
                    r.draft_id,
                    r.visibility.label(),
                    if r.encrypt { ", encrypted" } else { "" }
                );
            }
            Err(e) => self.status = format!("Compose failed: {e}"),
        }
        self.mode = Mode::Normal;
    }

    // ---- requests --------------------------------------------------------

    fn approve_request(&mut self) {
        if let Some(r) = self.selected_request().cloned() {
            // Local approval bookkeeping; wire-side Accept is a later phase.
            let _ = self.client.store.remove_request(&r.handle);
            self.status = format!("Approved {} (local)", r.handle);
            self.reload();
        }
    }

    fn reject_request(&mut self) {
        if let Some(r) = self.selected_request().cloned() {
            let _ = self.client.store.remove_request(&r.handle);
            self.status = format!("Rejected {}", r.handle);
            self.reload();
        }
    }

    fn open_selected(&mut self) {
        if self.view == View::Requests {
            return;
        }
        if let Some(p) = self.selected_post().cloned() {
            let _ = self.client.store.mark_read(&p.id);
            self.thread_replies = self.client.thread(&p.id).map(|(_, r)| r).unwrap_or_default();
            self.mode = Mode::Thread;
            self.status = if p.encrypted {
                "Decrypted locally (operator key)".into()
            } else {
                format!("Thread · {}", p.display_name())
            };
            self.reload();
        }
    }

    // ---- key handling ----------------------------------------------------

    /// Handle a key. `code`/`ch` come from the event loop; `ctrl` is the modifier.
    pub fn on_key(&mut self, key: Key) {
        // Overlays first.
        if self.show_help {
            // Any key closes help.
            self.show_help = false;
            return;
        }
        match self.mode {
            Mode::Composer => self.on_key_composer(key),
            Mode::Palette => self.on_key_palette(key),
            Mode::Thread => match key {
                Key::Esc | Key::Char('q') => self.mode = Mode::Normal,
                Key::Char('r') => self.open_composer(true),
                _ => {}
            },
            Mode::Normal => self.on_key_normal(key),
        }
    }

    fn on_key_normal(&mut self, key: Key) {
        // Leader (g) handling.
        if self.leader {
            self.leader = false;
            match key {
                Key::Char('h') => self.switch(View::Home),
                Key::Char('m') => self.switch(View::Mentions),
                Key::Char('r') => self.switch(View::Requests),
                Key::Char('d') => self.switch(View::Dms),
                Key::Char('n') => self.switch(View::Notifs),
                Key::Char('s') => self.switch(View::Sent),
                Key::Char('g') => {
                    self.selected = 0;
                }
                _ => {}
            }
            return;
        }

        match key {
            Key::Char('q') | Key::Esc => self.should_quit = true,
            Key::Char('j') | Key::Down => self.move_by(1),
            Key::Char('k') | Key::Up => self.move_by(-1),
            Key::PageDown | Key::CtrlChar('d') => self.move_by(10),
            Key::PageUp | Key::CtrlChar('u') => self.move_by(-10),
            Key::Char('G') => self.selected = self.current_len().saturating_sub(1),
            Key::Char('g') => self.leader = true,
            Key::Enter => self.open_selected(),
            Key::Char('c') => self.open_composer(false),
            Key::Char('r') if self.view != View::Requests => self.open_composer(true),
            Key::Char('m') => {
                if let Some(p) = self.selected_post().cloned() {
                    let _ = self.client.store.mark_read(&p.id);
                    self.reload();
                    self.status = "Marked read".into();
                }
            }
            Key::Char('A') if self.view == View::Requests => self.approve_request(),
            Key::Char('X') if self.view == View::Requests => self.reject_request(),
            Key::Char('/') => self.status = "Search: not yet wired".into(),
            Key::Char(':') | Key::CtrlChar('p') => {
                self.mode = Mode::Palette;
                self.palette.query.clear();
                self.palette.selected = 0;
            }
            Key::Char('?') => self.show_help = true,
            _ => {}
        }
    }

    fn on_key_composer(&mut self, key: Key) {
        match key {
            Key::Esc => {
                self.mode = Mode::Normal;
                self.status = "Compose cancelled".into();
            }
            Key::Enter => self.send_composer(),
            Key::CtrlChar('v') => self.composer.visibility = self.composer.visibility.cycle(),
            Key::CtrlChar('x') => self.composer.encrypt = !self.composer.encrypt,
            Key::Backspace => {
                self.composer.text.pop();
            }
            Key::Char(c) => self.composer.text.push(c),
            _ => {}
        }
    }

    fn on_key_palette(&mut self, key: Key) {
        let items = self.palette_items();
        match key {
            Key::Esc => {
                self.mode = Mode::Normal;
                self.palette.query.clear();
            }
            Key::Enter => {
                if let Some(item) = items.get(self.palette.selected) {
                    let action = item.action;
                    self.run_action(action);
                }
            }
            Key::Down => {
                if !items.is_empty() {
                    self.palette.selected = (self.palette.selected + 1).min(items.len() - 1);
                }
            }
            Key::Up => self.palette.selected = self.palette.selected.saturating_sub(1),
            Key::Backspace => {
                self.palette.query.pop();
                self.palette.selected = 0;
            }
            Key::Char(c) => {
                self.palette.query.push(c);
                self.palette.selected = 0;
            }
            _ => {}
        }
    }
}

/// A normalized key event (keeps ratatui/crossterm types out of `App`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    CtrlChar(char),
    Enter,
    Esc,
    Backspace,
    Up,
    Down,
    PageUp,
    PageDown,
}
