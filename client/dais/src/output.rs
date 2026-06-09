//! Output helpers: human-readable by default, `--json` opt-in, color gated on TTY +
//! `NO_COLOR`/`CLICOLOR` (CLIENT_REDESIGN.md P1–P3). We never switch shape on a pipe.

use std::io::IsTerminal;

use dais_client::model::Post;

/// Whether to emit ANSI color: TTY on stdout, unless `NO_COLOR` is set; forced on by
/// `CLICOLOR_FORCE`.
pub fn color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var_os("CLICOLOR_FORCE").is_some() {
        return true;
    }
    std::io::stdout().is_terminal()
}

pub struct Style {
    on: bool,
}

impl Style {
    pub fn new() -> Self {
        Style {
            on: color_enabled(),
        }
    }

    pub fn paint(&self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn dim(&self, s: &str) -> String {
        self.paint("2;37", s)
    }
    pub fn bold(&self, s: &str) -> String {
        self.paint("1", s)
    }
    pub fn cyan(&self, s: &str) -> String {
        self.paint("36", s)
    }
    pub fn green(&self, s: &str) -> String {
        self.paint("32", s)
    }
    pub fn yellow(&self, s: &str) -> String {
        self.paint("33", s)
    }
    pub fn magenta(&self, s: &str) -> String {
        self.paint("35", s)
    }
}

impl Default for Style {
    fn default() -> Self {
        Self::new()
    }
}

/// Render one post as a human-readable block.
pub fn print_post(p: &Post) {
    let s = Style::new();
    let dot = if p.unread { "●" } else { "○" };
    let star = if p.is_friend { "★" } else { " " };
    let vis = format!("{} {}", p.visibility.glyph(), p.visibility.label());
    let enc = if p.encrypted { " 🔒" } else { "" };
    println!(
        "{} {} {}  {}  {}{}  {}",
        s.cyan(dot),
        s.yellow(star),
        s.bold(p.display_name()),
        s.dim(&p.author_handle),
        vis_colored(&s, p, &vis),
        enc,
        s.dim(&dais_client::relative_time(p.published)),
    );
    println!("    {}", p.content);
    println!(
        "    {}",
        s.dim(&format!(
            "↳ {} · ♥ {} · ↗ {}  ·  {}",
            p.reply_count, p.like_count, p.boost_count, p.id
        ))
    );
    println!();
}

fn vis_colored(s: &Style, p: &Post, vis: &str) -> String {
    use dais_client::model::Visibility::*;
    match p.visibility {
        Public => s.yellow(vis),
        Followers => s.green(vis),
        Direct => s.magenta(vis),
    }
}

/// JSON shape for a post (stable for scripting).
pub fn post_json(p: &Post) -> serde_json::Value {
    serde_json::json!({
        "id": p.id,
        "author_handle": p.author_handle,
        "author_name": p.author_name,
        "content": p.content,
        "visibility": p.visibility.label(),
        "encrypted": p.encrypted,
        "published": p.published.to_rfc3339(),
        "in_reply_to": p.in_reply_to,
        "reply_count": p.reply_count,
        "like_count": p.like_count,
        "boost_count": p.boost_count,
        "is_friend": p.is_friend,
        "unread": p.unread,
    })
}
