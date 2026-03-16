/// Database migration system
///
/// Handles applying schema migrations to any database dialect

use crate::traits::{DatabaseProvider, DatabaseDialect};
use crate::error::{CoreResult, CoreError};
use crate::sql::convert_placeholders;
use serde_json::Value;

/// Migration metadata
#[derive(Debug, Clone)]
pub struct Migration {
    pub version: i32,
    pub name: String,
    pub up_sql: String,
    pub down_sql: Option<String>,
}

impl Migration {
    pub fn new(version: i32, name: impl Into<String>, up_sql: impl Into<String>) -> Self {
        Self {
            version,
            name: name.into(),
            up_sql: up_sql.into(),
            down_sql: None,
        }
    }

    pub fn with_down(mut self, down_sql: impl Into<String>) -> Self {
        self.down_sql = Some(down_sql.into());
        self
    }
}

/// Migration runner
pub struct MigrationRunner<'a> {
    db: &'a dyn DatabaseProvider,
}

impl<'a> MigrationRunner<'a> {
    pub fn new(db: &'a dyn DatabaseProvider) -> Self {
        Self { db }
    }

    /// Ensure the migrations table exists
    async fn ensure_migrations_table(&self) -> CoreResult<()> {
        let dialect = self.db.dialect();

        let create_table = match dialect {
            DatabaseDialect::SQLite => r#"
                CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
            "#,
            DatabaseDialect::PostgreSQL => r#"
                CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
            "#,
            DatabaseDialect::MySQL => r#"
                CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INT PRIMARY KEY,
                    name TEXT NOT NULL,
                    applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                )
            "#,
        };

        self.db.execute(create_table, &[]).await?;
        Ok(())
    }

    /// Get the current schema version
    pub async fn current_version(&self) -> CoreResult<i32> {
        self.ensure_migrations_table().await?;

        let sql = "SELECT MAX(version) as version FROM schema_migrations";
        let rows = self.db.execute(sql, &[]).await?;

        if rows.is_empty() {
            return Ok(0);
        }

        let version = rows[0]
            .get("version")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;

        Ok(version)
    }

    /// Check if a migration has been applied
    pub async fn is_applied(&self, version: i32) -> CoreResult<bool> {
        self.ensure_migrations_table().await?;

        let sql = convert_placeholders(
            "SELECT COUNT(*) as count FROM schema_migrations WHERE version = ?1",
            self.db.dialect()
        );

        let rows = self.db.execute(&sql, &[Value::Number(version.into())]).await?;

        if rows.is_empty() {
            return Ok(false);
        }

        let count = rows[0]
            .get("count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        Ok(count > 0)
    }

    /// Apply a migration
    pub async fn apply(&self, migration: &Migration) -> CoreResult<()> {
        // Check if already applied
        if self.is_applied(migration.version).await? {
            return Ok(());
        }

        // Convert SQL placeholders for the target dialect
        let sql = convert_placeholders(&migration.up_sql, self.db.dialect());

        // Split on semicolons and execute each statement
        for statement in sql.split(';') {
            let statement = statement.trim();
            if statement.is_empty() {
                continue;
            }

            self.db.execute(statement, &[]).await.map_err(|e| {
                CoreError::Internal(format!(
                    "Migration {} failed: {}",
                    migration.version, e
                ))
            })?;
        }

        // Record migration
        let record_sql = convert_placeholders(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            self.db.dialect()
        );

        self.db.execute(&record_sql, &[
            Value::Number(migration.version.into()),
            Value::String(migration.name.clone()),
        ]).await?;

        Ok(())
    }

    /// Apply all pending migrations
    pub async fn migrate(&self, migrations: &[Migration]) -> CoreResult<()> {
        self.ensure_migrations_table().await?;

        for migration in migrations {
            if !self.is_applied(migration.version).await? {
                println!("Applying migration {}: {}", migration.version, migration.name);
                self.apply(migration).await?;
            }
        }

        Ok(())
    }

    /// Rollback a migration
    pub async fn rollback(&self, migration: &Migration) -> CoreResult<()> {
        // Check if applied
        if !self.is_applied(migration.version).await? {
            return Ok(());
        }

        // Get down SQL
        let down_sql = migration.down_sql.as_ref()
            .ok_or_else(|| CoreError::Internal(
                format!("Migration {} has no down migration", migration.version)
            ))?;

        // Convert SQL placeholders for the target dialect
        let sql = convert_placeholders(down_sql, self.db.dialect());

        // Split on semicolons and execute each statement
        for statement in sql.split(';') {
            let statement = statement.trim();
            if statement.is_empty() {
                continue;
            }

            self.db.execute(statement, &[]).await.map_err(|e| {
                CoreError::Internal(format!(
                    "Rollback of migration {} failed: {}",
                    migration.version, e
                ))
            })?;
        }

        // Remove migration record
        let delete_sql = convert_placeholders(
            "DELETE FROM schema_migrations WHERE version = ?1",
            self.db.dialect()
        );

        self.db.execute(&delete_sql, &[
            Value::Number(migration.version.into()),
        ]).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_creation() {
        let migration = Migration::new(
            1,
            "initial_schema",
            "CREATE TABLE users (id INTEGER PRIMARY KEY)"
        );

        assert_eq!(migration.version, 1);
        assert_eq!(migration.name, "initial_schema");
        assert!(migration.down_sql.is_none());
    }
}
