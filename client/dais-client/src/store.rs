//! Local SQLite store — the on-disk mirror that makes the client instant and
//! offline-capable (CLIENT_REDESIGN.md §3.2). The home timeline is *ingested* into
//! `timeline_posts`; unread tracking lives here, not on the server.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::Result;
use crate::model::{Feed, FollowRequest, Post, Visibility};

/// A handle to the local store.
pub struct Store {
    conn: Connection,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS timeline_posts (
    id           TEXT PRIMARY KEY,
    feed         TEXT NOT NULL DEFAULT 'home',
    author_handle TEXT NOT NULL,
    author_name  TEXT,
    content      TEXT NOT NULL,
    visibility   TEXT NOT NULL,
    encrypted    INTEGER NOT NULL DEFAULT 0,
    published    TEXT NOT NULL,
    in_reply_to  TEXT,
    reply_count  INTEGER NOT NULL DEFAULT 0,
    like_count   INTEGER NOT NULL DEFAULT 0,
    boost_count  INTEGER NOT NULL DEFAULT 0,
    is_friend    INTEGER NOT NULL DEFAULT 0,
    unread       INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_timeline_feed_pub
    ON timeline_posts(feed, published DESC);

CREATE TABLE IF NOT EXISTS follow_requests (
    handle           TEXT PRIMARY KEY,
    name             TEXT,
    message          TEXT,
    asked_at         TEXT NOT NULL,
    mutuals          INTEGER NOT NULL DEFAULT 0,
    account_age_days INTEGER,
    post_count       INTEGER,
    unread           INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS drafts (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    content    TEXT NOT NULL,
    visibility TEXT NOT NULL,
    encrypt    INTEGER NOT NULL DEFAULT 0,
    in_reply_to TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

impl Store {
    /// Open (creating if needed) the store at `path`, running migrations.
    pub fn open(path: &std::path::Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    /// In-memory store, for tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    // ---- timeline ---------------------------------------------------------

    /// Insert or replace a post (dedupe on `id`, per #63).
    pub fn upsert_post(&self, feed: Feed, p: &Post) -> Result<()> {
        self.conn.execute(
            "INSERT INTO timeline_posts
                (id, feed, author_handle, author_name, content, visibility, encrypted,
                 published, in_reply_to, reply_count, like_count, boost_count, is_friend, unread)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
             ON CONFLICT(id) DO UPDATE SET
                content=excluded.content,
                reply_count=excluded.reply_count,
                like_count=excluded.like_count,
                boost_count=excluded.boost_count,
                is_friend=excluded.is_friend",
            params![
                p.id,
                feed.as_str(),
                p.author_handle,
                p.author_name,
                p.content,
                p.visibility.as_str(),
                p.encrypted as i64,
                p.published.to_rfc3339(),
                p.in_reply_to,
                p.reply_count,
                p.like_count,
                p.boost_count,
                p.is_friend as i64,
                p.unread as i64,
            ],
        )?;
        Ok(())
    }

    /// Read a feed, newest first.
    pub fn timeline(&self, feed: Feed, limit: usize) -> Result<Vec<Post>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, author_handle, author_name, content, visibility, encrypted,
                    published, in_reply_to, reply_count, like_count, boost_count,
                    is_friend, unread
             FROM timeline_posts WHERE feed = ?1
             ORDER BY published DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![feed.as_str(), limit as i64], row_to_post)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Replies to a post id, oldest first (for the thread pane).
    pub fn replies(&self, parent_id: &str) -> Result<Vec<Post>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, author_handle, author_name, content, visibility, encrypted,
                    published, in_reply_to, reply_count, like_count, boost_count,
                    is_friend, unread
             FROM timeline_posts WHERE in_reply_to = ?1
             ORDER BY published ASC",
        )?;
        let rows = stmt
            .query_map(params![parent_id], row_to_post)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_post(&self, id: &str) -> Result<Option<Post>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, author_handle, author_name, content, visibility, encrypted,
                    published, in_reply_to, reply_count, like_count, boost_count,
                    is_friend, unread
             FROM timeline_posts WHERE id = ?1",
        )?;
        Ok(stmt.query_row(params![id], row_to_post).optional()?)
    }

    pub fn mark_read(&self, id: &str) -> Result<()> {
        self.conn
            .execute("UPDATE timeline_posts SET unread = 0 WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn unread_count(&self, feed: Feed) -> Result<u32> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM timeline_posts WHERE feed = ?1 AND unread = 1",
            params![feed.as_str()],
            |r| r.get(0),
        )?;
        Ok(n as u32)
    }

    // ---- follow requests --------------------------------------------------

    pub fn upsert_request(&self, r: &FollowRequest) -> Result<()> {
        self.conn.execute(
            "INSERT INTO follow_requests
                (handle, name, message, asked_at, mutuals, account_age_days, post_count, unread)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(handle) DO UPDATE SET
                message=excluded.message, mutuals=excluded.mutuals",
            params![
                r.handle,
                r.name,
                r.message,
                r.asked_at.to_rfc3339(),
                r.mutuals,
                r.account_age_days,
                r.post_count,
                r.unread as i64,
            ],
        )?;
        Ok(())
    }

    pub fn requests(&self) -> Result<Vec<FollowRequest>> {
        let mut stmt = self.conn.prepare(
            "SELECT handle, name, message, asked_at, mutuals, account_age_days, post_count, unread
             FROM follow_requests ORDER BY asked_at DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(FollowRequest {
                    handle: row.get(0)?,
                    name: row.get(1)?,
                    message: row.get(2)?,
                    asked_at: parse_dt(row, 3)?,
                    mutuals: row.get::<_, i64>(4)? as u32,
                    account_age_days: row.get::<_, Option<i64>>(5)?.map(|v| v as u32),
                    post_count: row.get::<_, Option<i64>>(6)?.map(|v| v as u32),
                    unread: row.get::<_, i64>(7)? != 0,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn remove_request(&self, handle: &str) -> Result<bool> {
        let n = self
            .conn
            .execute("DELETE FROM follow_requests WHERE handle = ?1", params![handle])?;
        Ok(n > 0)
    }

    // ---- drafts -----------------------------------------------------------

    pub fn save_draft(
        &self,
        content: &str,
        visibility: Visibility,
        encrypt: bool,
        in_reply_to: Option<&str>,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO drafts (content, visibility, encrypt, in_reply_to, created_at)
             VALUES (?1,?2,?3,?4,?5)",
            params![
                content,
                visibility.as_str(),
                encrypt as i64,
                in_reply_to,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    // ---- meta -------------------------------------------------------------

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()?)
    }
}

fn parse_dt(row: &Row, idx: usize) -> rusqlite::Result<DateTime<Utc>> {
    let s: String = row.get(idx)?;
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e)))
}

fn row_to_post(row: &Row) -> rusqlite::Result<Post> {
    let vis: String = row.get(4)?;
    Ok(Post {
        id: row.get(0)?,
        author_handle: row.get(1)?,
        author_name: row.get(2)?,
        content: row.get(3)?,
        visibility: Visibility::parse(&vis).unwrap_or(Visibility::Followers),
        encrypted: row.get::<_, i64>(5)? != 0,
        published: parse_dt(row, 6)?,
        in_reply_to: row.get(7)?,
        reply_count: row.get::<_, i64>(8)? as u32,
        like_count: row.get::<_, i64>(9)? as u32,
        boost_count: row.get::<_, i64>(10)? as u32,
        is_friend: row.get::<_, i64>(11)? != 0,
        unread: row.get::<_, i64>(12)? != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> Post {
        Post {
            id: id.to_string(),
            author_handle: "@alice@coolhost.social".into(),
            author_name: Some("Alice".into()),
            content: "hello".into(),
            visibility: Visibility::Followers,
            encrypted: false,
            published: Utc::now(),
            in_reply_to: None,
            reply_count: 0,
            like_count: 0,
            boost_count: 0,
            is_friend: true,
            unread: true,
        }
    }

    #[test]
    fn upsert_and_read_and_unread() {
        let store = Store::open_in_memory().unwrap();
        store.upsert_post(Feed::Home, &sample("a")).unwrap();
        store.upsert_post(Feed::Home, &sample("b")).unwrap();
        assert_eq!(store.timeline(Feed::Home, 10).unwrap().len(), 2);
        assert_eq!(store.unread_count(Feed::Home).unwrap(), 2);
        store.mark_read("a").unwrap();
        assert_eq!(store.unread_count(Feed::Home).unwrap(), 1);

        // upsert dedupes on id
        store.upsert_post(Feed::Home, &sample("a")).unwrap();
        assert_eq!(store.timeline(Feed::Home, 10).unwrap().len(), 2);
    }
}
