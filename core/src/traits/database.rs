/// Database abstraction trait for platform-agnostic database operations
///
/// Implementations:
/// - Cloudflare: D1 (SQLite)
/// - Vercel: Neon Postgres
/// - Netlify: Neon Postgres or Turso
/// - Railway: PostgreSQL

use super::{PlatformResult, Row, Statement};
use async_trait::async_trait;
use serde_json::Value;

#[async_trait(?Send)]
pub trait DatabaseProvider {
    /// Execute a single SQL query with parameters
    ///
    /// # Example
    /// ```rust,ignore
    /// let rows = db.execute(
    ///     "SELECT * FROM posts WHERE id = ?",
    ///     &[Value::String("123".into())]
    /// ).await?;
    /// ```
    async fn execute(&self, sql: &str, params: &[Value]) -> PlatformResult<Vec<Row>>;

    /// Execute multiple statements in a batch
    ///
    /// Useful for bulk inserts or migrations
    async fn batch(&self, statements: Vec<Statement>) -> PlatformResult<()>;

    /// Get the SQL dialect for this database
    ///
    /// Used to generate platform-specific SQL when needed
    fn dialect(&self) -> DatabaseDialect;
}

/// SQL dialect variations between platforms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseDialect {
    /// SQLite (Cloudflare D1, Turso)
    SQLite,

    /// PostgreSQL (Neon, Railway, Supabase)
    PostgreSQL,

    /// MySQL (PlanetScale, Vitess)
    MySQL,
}

impl DatabaseDialect {
    /// Get the SQL for RETURNING clause (or equivalent)
    ///
    /// SQLite: Use last_insert_rowid()
    /// PostgreSQL: RETURNING id
    /// MySQL: No native RETURNING, use SELECT LAST_INSERT_ID()
    pub fn returning_clause(&self, column: &str) -> String {
        match self {
            DatabaseDialect::SQLite => format!("/* Use last_insert_rowid() */"),
            DatabaseDialect::PostgreSQL => format!("RETURNING {}", column),
            DatabaseDialect::MySQL => format!("/* Use SELECT LAST_INSERT_ID() */"),
        }
    }

    /// Get placeholder syntax for parameters
    ///
    /// SQLite: ?1, ?2, ?3
    /// PostgreSQL: $1, $2, $3
    /// MySQL: ?, ?, ?
    pub fn placeholder(&self, index: usize) -> String {
        match self {
            DatabaseDialect::SQLite => format!("?{}", index),
            DatabaseDialect::PostgreSQL => format!("${}", index),
            DatabaseDialect::MySQL => "?".to_string(),
        }
    }

    /// Get current timestamp function
    ///
    /// SQLite: datetime('now')
    /// PostgreSQL: CURRENT_TIMESTAMP
    /// MySQL: CURRENT_TIMESTAMP
    pub fn now_function(&self) -> &'static str {
        match self {
            DatabaseDialect::SQLite => "datetime('now')",
            DatabaseDialect::PostgreSQL => "CURRENT_TIMESTAMP",
            DatabaseDialect::MySQL => "CURRENT_TIMESTAMP",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialect_placeholders() {
        assert_eq!(DatabaseDialect::SQLite.placeholder(1), "?1");
        assert_eq!(DatabaseDialect::PostgreSQL.placeholder(1), "$1");
        assert_eq!(DatabaseDialect::MySQL.placeholder(1), "?");
    }

    #[test]
    fn test_dialect_now() {
        assert_eq!(DatabaseDialect::SQLite.now_function(), "datetime('now')");
        assert_eq!(DatabaseDialect::PostgreSQL.now_function(), "CURRENT_TIMESTAMP");
    }
}
