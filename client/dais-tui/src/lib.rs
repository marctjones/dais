//! dais-tui — "a mail client for the fediverse" (CLIENT_REDESIGN.md §5).
//!
//! A Ratatui front-end over the `dais-client` SDK. Owns the event loop and redraws
//! each frame from a single [`app::App`] state; reads come from the local store so
//! keypresses never block (P6).

pub mod app;
pub mod ui;

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use dais_client::Client;

use app::{App, Key};

/// Launch the TUI, taking over the terminal until the user quits.
pub fn run(client: Client) -> Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, client);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, client: Client) -> Result<()> {
    let mut app = App::new(client);
    while !app.should_quit {
        terminal.draw(|f| ui::render(f, &app))?;

        // Poll so the loop can later service background refreshes; for now keypresses
        // drive everything and reads are instant from the local store.
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    if let Some(key) = translate(k.code, k.modifiers) {
                        app.on_key(key);
                    }
                }
            }
        }
    }
    Ok(())
}

fn translate(code: KeyCode, mods: KeyModifiers) -> Option<Key> {
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    Some(match code {
        KeyCode::Char(c) if ctrl => Key::CtrlChar(c.to_ascii_lowercase()),
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Mode, View};
    use dais_client::model::Visibility;
    use dais_client::{Config, Store};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn demo_app() -> App {
        let store = Store::open_in_memory().unwrap();
        let mut cfg = Config::default();
        cfg.handle = Some("@social@dais.social".into());
        let client = Client::from_parts(cfg, store);
        client.seed_demo().unwrap();
        App::new(client)
    }

    /// Render the current app to an in-memory terminal and return the screen text.
    fn screen(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(110, 32)).unwrap();
        terminal.draw(|f| ui::render(f, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    fn keys(app: &mut App, s: &str) {
        for ch in s.chars() {
            app.on_key(Key::Char(ch));
        }
    }

    // ---- rendering -------------------------------------------------------

    #[test]
    fn renders_home_feed_with_all_authors() {
        let text = screen(&demo_app());
        assert!(text.contains("dais"), "title missing");
        assert!(text.contains("Alice"), "Alice post missing");
        assert!(text.contains("Bob Martinez"), "Bob post missing");
        assert!(text.contains("Carol"), "Carol post missing");
    }

    #[test]
    fn header_shows_tabs_and_unread_badges() {
        let text = screen(&demo_app());
        for tab in ["Home", "Mentions", "Requests", "DMs", "Sent", "Notifs"] {
            assert!(text.contains(tab), "header missing tab {tab}");
        }
        // 2 unread home posts, 1 unread request → badges.
        assert!(text.contains("Home(2)"), "home unread badge missing: {text:?}");
        assert!(text.contains("Requests(1)"), "requests unread badge missing");
    }

    #[test]
    fn privacy_glyphs_and_encryption_render() {
        let text = screen(&demo_app());
        assert!(text.contains('🌐'), "public glyph missing");
        assert!(text.contains('👥'), "followers glyph missing");
        assert!(text.contains('🔒'), "encryption lock missing");
        assert!(text.contains('★'), "friend star missing");
    }

    // ---- navigation ------------------------------------------------------

    #[test]
    fn jk_moves_selection() {
        let mut app = demo_app();
        assert_eq!(app.selected, 0);
        app.on_key(Key::Char('j'));
        assert_eq!(app.selected, 1);
        app.on_key(Key::Char('j'));
        assert_eq!(app.selected, 2);
        app.on_key(Key::Char('k'));
        assert_eq!(app.selected, 1);
        app.on_key(Key::Char('G'));
        assert_eq!(app.selected, app.current_len() - 1);
    }

    #[test]
    fn leader_switches_every_view() {
        let cases = [
            ('m', View::Mentions),
            ('r', View::Requests),
            ('d', View::Dms),
            ('n', View::Notifs),
            ('s', View::Sent),
            ('h', View::Home),
        ];
        for (k, want) in cases {
            let mut app = demo_app();
            app.on_key(Key::Char('g'));
            app.on_key(Key::Char(k));
            assert_eq!(app.view, want, "g {k} should switch view");
        }
    }

    #[test]
    fn requests_view_renders_approval_inbox() {
        let mut app = demo_app();
        keys(&mut app, "gr");
        let text = screen(&app);
        assert!(text.contains("Dave Park"));
        assert!(text.contains("approve"));
        assert!(text.contains("reject"));
    }

    #[test]
    fn empty_feeds_show_placeholder() {
        let mut app = demo_app();
        keys(&mut app, "gs"); // Sent — empty
        assert!(screen(&app).contains("No posts"), "sent empty-state missing");
        keys(&mut app, "gd"); // DMs — later-phase placeholder
        assert!(screen(&app).contains("later phase"), "dms placeholder missing");
    }

    // ---- reading model ---------------------------------------------------

    #[test]
    fn opening_a_post_marks_read_and_shows_thread() {
        let mut app = demo_app();
        app.on_key(Key::Char('j')); // select Alice (index 1, has a reply)
        app.on_key(Key::Enter);
        assert_eq!(app.mode, Mode::Thread);
        let text = screen(&app);
        assert!(text.contains("Thread"), "thread pane title missing");
        assert!(text.contains("1 replies"), "reply count missing");
        assert!(text.contains("Same, can't wait"), "reply body missing");
    }

    #[test]
    fn mark_read_decrements_unread_badge() {
        let mut app = demo_app();
        app.on_key(Key::Char('j')); // Alice (unread)
        app.on_key(Key::Char('m'));
        assert!(screen(&app).contains("Home(1)"), "unread badge should drop to 1");
    }

    // ---- composer --------------------------------------------------------

    #[test]
    fn composer_renders_audience_and_public_warning() {
        let mut app = demo_app();
        app.on_key(Key::Char('c'));
        assert_eq!(app.mode, Mode::Composer);
        let text = screen(&app);
        assert!(text.contains("Compose"));
        assert!(text.contains("Audience"));
        assert!(text.contains("followers"), "default audience should be followers");

        app.on_key(Key::CtrlChar('v')); // Followers → Public
        assert_eq!(app.composer.visibility, Visibility::Public);
        assert!(
            screen(&app).contains("federate"),
            "public federation warning missing"
        );
    }

    #[test]
    fn composer_encrypt_toggle_shows_fallback_note() {
        let mut app = demo_app();
        app.on_key(Key::Char('c'));
        app.on_key(Key::CtrlChar('x'));
        assert!(app.composer.encrypt);
        let text = screen(&app);
        assert!(text.contains("on"), "encrypt [ on ] state missing");
        assert!(text.contains("open in dais"), "fallback note missing");
    }

    #[test]
    fn visibility_cycles_through_all_three() {
        let mut app = demo_app();
        app.on_key(Key::Char('c'));
        assert_eq!(app.composer.visibility, Visibility::Followers);
        app.on_key(Key::CtrlChar('v'));
        assert_eq!(app.composer.visibility, Visibility::Public);
        app.on_key(Key::CtrlChar('v'));
        assert_eq!(app.composer.visibility, Visibility::Direct);
        app.on_key(Key::CtrlChar('v'));
        assert_eq!(app.composer.visibility, Visibility::Followers);
    }

    #[test]
    fn composer_typing_and_send_stages_draft() {
        let mut app = demo_app();
        app.on_key(Key::Char('c'));
        keys(&mut app, "hi there");
        assert_eq!(app.composer.text, "hi there");
        app.on_key(Key::Enter); // send
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status.to_lowercase().contains("draft"), "status: {}", app.status);
    }

    #[test]
    fn reply_prefills_recipient() {
        let mut app = demo_app();
        app.on_key(Key::Char('j')); // Alice
        app.on_key(Key::Char('r')); // reply
        assert_eq!(app.mode, Mode::Composer);
        assert_eq!(app.composer.reply_handle.as_deref(), Some("@alice@coolhost.social"));
        assert!(screen(&app).contains("Replying to"));
    }

    // ---- palette + help --------------------------------------------------

    #[test]
    fn palette_opens_and_filters() {
        let mut app = demo_app();
        app.on_key(Key::Char(':'));
        assert_eq!(app.mode, Mode::Palette);
        assert!(screen(&app).contains("Command palette"));
        keys(&mut app, "req");
        let text = screen(&app);
        assert!(text.contains("Go: Requests"), "filtered item missing");
        assert!(!text.contains("Compose post"), "non-matching item should be filtered out");
    }

    #[test]
    fn palette_runs_selected_action() {
        let mut app = demo_app();
        app.on_key(Key::Char(':'));
        keys(&mut app, "requests");
        app.on_key(Key::Enter);
        assert_eq!(app.view, View::Requests, "palette action should switch view");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn help_overlay_renders_and_any_key_closes() {
        let mut app = demo_app();
        app.on_key(Key::Char('?'));
        assert!(app.show_help);
        assert!(screen(&app).contains("keybindings"));
        app.on_key(Key::Char('x')); // any key closes
        assert!(!app.show_help);
    }

    // ---- requests actions + quit ----------------------------------------

    #[test]
    fn approve_removes_request() {
        let mut app = demo_app();
        keys(&mut app, "gr");
        assert_eq!(app.requests.len(), 1);
        app.on_key(Key::Char('A'));
        assert_eq!(app.requests.len(), 0);
        assert!(screen(&app).contains("No pending"), "empty inbox state missing");
    }

    #[test]
    fn reject_removes_request() {
        let mut app = demo_app();
        keys(&mut app, "gr");
        app.on_key(Key::Char('X'));
        assert_eq!(app.requests.len(), 0);
    }

    #[test]
    fn q_quits() {
        let mut app = demo_app();
        assert!(!app.should_quit);
        app.on_key(Key::Char('q'));
        assert!(app.should_quit);
    }
}
