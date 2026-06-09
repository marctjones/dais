//! End-to-end GUI tests for the TUI.
//!
//! Unlike the headless `TestBackend` tests in `dais-tui`, these spawn the **real
//! `dais tui` binary** under a pseudo-terminal, send actual keystrokes, and parse the
//! emitted terminal escape sequences with a VT100 parser into a virtual screen — the
//! closest thing to a human driving it. Unix-only (PTY).

#![cfg(unix)]

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

const ROWS: u16 = 32;
const COLS: u16 = 110;

/// A live TUI process attached to a PTY, with its output parsed into a screen grid.
struct Tui {
    writer: Box<dyn Write + Send>,
    parser: Arc<Mutex<vt100::Parser>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    _master: Box<dyn portable_pty::MasterPty + Send>,
}

impl Tui {
    fn launch(config: &str, store: &str) -> Tui {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: ROWS,
                cols: COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");

        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_dais"));
        cmd.arg("tui");
        cmd.env("DAIS_CONFIG", config);
        cmd.env("DAIS_STORE", store);
        cmd.env("TERM", "xterm-256color");

        let child = pair.slave.spawn_command(cmd).expect("spawn dais tui");
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().expect("reader");
        let writer = pair.master.take_writer().expect("writer");
        let parser = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 0)));

        // Pump PTY output into the VT parser on a background thread.
        let sink = parser.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => sink.lock().unwrap().process(&buf[..n]),
                }
            }
        });

        Tui {
            writer,
            parser,
            child,
            _master: pair.master,
        }
    }

    /// Current visible screen, as text.
    fn screen(&self) -> String {
        self.parser.lock().unwrap().screen().contents()
    }

    fn send(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("write keys");
        self.writer.flush().expect("flush");
    }

    /// Poll the screen until `needle` appears or `timeout` elapses.
    fn wait_for(&self, needle: &str, timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.screen().contains(needle) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        false
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Seed an isolated config + store for one test, returning their paths.
fn setup(name: &str) -> (String, String) {
    let dir: PathBuf = std::env::temp_dir().join(format!("dais-pty-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir temp");
    let config = dir.join("config.toml");
    let store = dir.join("store.db");

    let status = std::process::Command::new(env!("CARGO_BIN_EXE_dais"))
        .args(["init", "--demo", "--handle", "@social@dais.social"])
        .env("DAIS_CONFIG", &config)
        .env("DAIS_STORE", &store)
        .status()
        .expect("run dais init");
    assert!(status.success(), "dais init --demo failed");

    (
        config.to_string_lossy().into_owned(),
        store.to_string_lossy().into_owned(),
    )
}

const T: Duration = Duration::from_secs(8);

#[test]
fn tui_boots_and_renders_home_feed() {
    let (config, store) = setup("boot");
    let mut tui = Tui::launch(&config, &store);
    assert!(
        tui.wait_for("Alice", T),
        "home feed never rendered; screen was:\n{}",
        tui.screen()
    );
    let screen = tui.screen();
    assert!(screen.contains("Home"), "header missing");
    assert!(screen.contains("Bob Martinez"), "second post missing");
    tui.send(b"q");
}

#[test]
fn tui_leader_key_navigates_to_requests_inbox() {
    let (config, store) = setup("nav");
    let mut tui = Tui::launch(&config, &store);
    assert!(tui.wait_for("Alice", T), "boot failed:\n{}", tui.screen());
    tui.send(b"gr"); // leader g → r = Requests
    assert!(
        tui.wait_for("Dave Park", T),
        "requests view never rendered:\n{}",
        tui.screen()
    );
    tui.send(b"q");
}

#[test]
fn tui_composer_opens_with_privacy_controls() {
    let (config, store) = setup("composer");
    let mut tui = Tui::launch(&config, &store);
    assert!(tui.wait_for("Alice", T), "boot failed:\n{}", tui.screen());
    tui.send(b"c"); // compose
    assert!(
        tui.wait_for("Compose", T),
        "composer never opened:\n{}",
        tui.screen()
    );
    assert!(tui.screen().contains("Audience"), "audience control missing");
    tui.send(b"\x18"); // Ctrl-X → toggle encrypt
    assert!(
        tui.wait_for("open in dais", T),
        "encrypt fallback note missing:\n{}",
        tui.screen()
    );
    tui.send(b"\x1b"); // Esc out of composer
    tui.send(b"q");
}

#[test]
fn tui_help_overlay_and_quit() {
    let (config, store) = setup("help");
    let mut tui = Tui::launch(&config, &store);
    assert!(tui.wait_for("Alice", T), "boot failed:\n{}", tui.screen());
    tui.send(b"?");
    assert!(
        tui.wait_for("keybindings", T),
        "help overlay missing:\n{}",
        tui.screen()
    );
    tui.send(b"x"); // any key closes help
    tui.send(b"q"); // quit
    // Give the process a moment to tear down the alternate screen and exit.
    std::thread::sleep(Duration::from_millis(300));
}
