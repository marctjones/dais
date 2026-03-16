/// SQL abstraction layer for portable queries across database dialects
///
/// This module provides utilities to write SQL queries that work across
/// SQLite (Cloudflare D1) and PostgreSQL (Vercel Postgres, Supabase, etc.)

pub mod query;
pub mod schema;

pub use query::QueryBuilder;
pub use schema::SchemaBuilder;

use crate::traits::DatabaseDialect;

/// Convert parameter placeholders based on database dialect
///
/// # Examples
///
/// ```
/// // SQLite uses ?1, ?2, ?3
/// let sql = "SELECT * FROM users WHERE id = ?1 AND name = ?2";
/// let pg_sql = convert_placeholders(sql, DatabaseDialect::PostgreSQL);
/// assert_eq!(pg_sql, "SELECT * FROM users WHERE id = $1 AND name = $2");
/// ```
pub fn convert_placeholders(sql: &str, dialect: DatabaseDialect) -> String {
    match dialect {
        DatabaseDialect::SQLite => sql.to_string(),
        DatabaseDialect::PostgreSQL => {
            // Convert ?1, ?2, ?3 to $1, $2, $3
            let mut result = sql.to_string();
            for i in (1..=20).rev() {
                result = result.replace(&format!("?{}", i), &format!("${}", i));
            }
            result
        }
        DatabaseDialect::MySQL => {
            // Convert ?1, ?2, ?3 to ? for MySQL
            let mut result = sql.to_string();
            for i in (1..=20).rev() {
                result = result.replace(&format!("?{}", i), "?");
            }
            result
        }
    }
}

/// Get the appropriate RETURNING clause for the database dialect
pub fn returning_clause(dialect: DatabaseDialect, columns: &[&str]) -> String {
    match dialect {
        DatabaseDialect::SQLite => {
            // SQLite doesn't support RETURNING in most cases
            String::new()
        }
        DatabaseDialect::PostgreSQL => {
            if columns.is_empty() {
                String::new()
            } else {
                format!(" RETURNING {}", columns.join(", "))
            }
        }
        DatabaseDialect::MySQL => {
            // MySQL doesn't support RETURNING
            String::new()
        }
    }
}

/// Get the appropriate auto-increment column definition
pub fn auto_increment_column(dialect: DatabaseDialect, column_name: &str) -> String {
    match dialect {
        DatabaseDialect::SQLite => {
            format!("{} INTEGER PRIMARY KEY AUTOINCREMENT", column_name)
        }
        DatabaseDialect::PostgreSQL => {
            format!("{} SERIAL PRIMARY KEY", column_name)
        }
        DatabaseDialect::MySQL => {
            format!("{} INT AUTO_INCREMENT PRIMARY KEY", column_name)
        }
    }
}

/// Get the appropriate timestamp default value
pub fn timestamp_default(dialect: DatabaseDialect) -> &'static str {
    match dialect {
        DatabaseDialect::SQLite => "CURRENT_TIMESTAMP",
        DatabaseDialect::PostgreSQL => "CURRENT_TIMESTAMP",
        DatabaseDialect::MySQL => "CURRENT_TIMESTAMP",
    }
}

/// Get the appropriate boolean type
pub fn boolean_type(dialect: DatabaseDialect) -> &'static str {
    match dialect {
        DatabaseDialect::SQLite => "INTEGER", // SQLite uses 0/1 for booleans
        DatabaseDialect::PostgreSQL => "BOOLEAN",
        DatabaseDialect::MySQL => "TINYINT(1)", // MySQL uses TINYINT for booleans
    }
}

/// Get the appropriate JSON type
pub fn json_type(dialect: DatabaseDialect) -> &'static str {
    match dialect {
        DatabaseDialect::SQLite => "TEXT", // SQLite stores JSON as TEXT
        DatabaseDialect::PostgreSQL => "JSONB",
        DatabaseDialect::MySQL => "JSON",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_placeholders_sqlite() {
        let sql = "SELECT * FROM users WHERE id = ?1";
        let result = convert_placeholders(sql, DatabaseDialect::SQLite);
        assert_eq!(result, sql);
    }

    #[test]
    fn test_convert_placeholders_postgres() {
        let sql = "SELECT * FROM users WHERE id = ?1 AND name = ?2";
        let result = convert_placeholders(sql, DatabaseDialect::PostgreSQL);
        assert_eq!(result, "SELECT * FROM users WHERE id = $1 AND name = $2");
    }

    #[test]
    fn test_auto_increment_sqlite() {
        let col = auto_increment_column(DatabaseDialect::SQLite, "id");
        assert_eq!(col, "id INTEGER PRIMARY KEY AUTOINCREMENT");
    }

    #[test]
    fn test_auto_increment_postgres() {
        let col = auto_increment_column(DatabaseDialect::PostgreSQL, "id");
        assert_eq!(col, "id SERIAL PRIMARY KEY");
    }
}
