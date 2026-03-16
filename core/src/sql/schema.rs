/// Schema builder for portable database migrations
///
/// Provides utilities to define database schemas that work across dialects

use crate::traits::DatabaseDialect;
use super::{auto_increment_column, boolean_type, json_type, timestamp_default};

pub struct SchemaBuilder {
    dialect: DatabaseDialect,
}

impl SchemaBuilder {
    pub fn new(dialect: DatabaseDialect) -> Self {
        Self { dialect }
    }

    /// Generate CREATE TABLE statement
    pub fn create_table(&self, name: &str, columns: &[ColumnDef]) -> String {
        let mut sql = format!("CREATE TABLE IF NOT EXISTS {} (\n", name);

        let column_defs: Vec<String> = columns.iter().map(|col| {
            self.column_definition(col)
        }).collect();

        sql.push_str(&format!("  {}\n", column_defs.join(",\n  ")));
        sql.push(')');

        sql
    }

    /// Generate a column definition
    fn column_definition(&self, col: &ColumnDef) -> String {
        let mut def = col.name.clone();

        // Handle auto-increment specially
        if col.auto_increment {
            return auto_increment_column(self.dialect, &col.name);
        }

        // Add type
        def.push(' ');
        def.push_str(&self.column_type(&col.col_type));

        // Add constraints
        if col.primary_key {
            def.push_str(" PRIMARY KEY");
        }
        if col.not_null {
            def.push_str(" NOT NULL");
        }
        if col.unique {
            def.push_str(" UNIQUE");
        }
        if let Some(ref default) = col.default {
            def.push_str(&format!(" DEFAULT {}", default));
        }

        def
    }

    /// Get the SQL type for a column
    fn column_type(&self, col_type: &ColumnType) -> String {
        match col_type {
            ColumnType::Text => "TEXT".to_string(),
            ColumnType::Integer => "INTEGER".to_string(),
            ColumnType::BigInt => match self.dialect {
                DatabaseDialect::SQLite => "INTEGER".to_string(),
                DatabaseDialect::PostgreSQL => "BIGINT".to_string(),
                DatabaseDialect::MySQL => "BIGINT".to_string(),
            },
            ColumnType::Real => match self.dialect {
                DatabaseDialect::SQLite => "REAL".to_string(),
                DatabaseDialect::PostgreSQL => "DOUBLE PRECISION".to_string(),
                DatabaseDialect::MySQL => "DOUBLE".to_string(),
            },
            ColumnType::Boolean => boolean_type(self.dialect).to_string(),
            ColumnType::Json => json_type(self.dialect).to_string(),
            ColumnType::Timestamp => "TIMESTAMP".to_string(),
            ColumnType::Uuid => match self.dialect {
                DatabaseDialect::SQLite => "TEXT".to_string(),
                DatabaseDialect::PostgreSQL => "UUID".to_string(),
                DatabaseDialect::MySQL => "CHAR(36)".to_string(),
            },
        }
    }

    /// Generate CREATE INDEX statement
    pub fn create_index(&self, index_name: &str, table: &str, columns: &[&str], unique: bool) -> String {
        let unique_clause = if unique { "UNIQUE " } else { "" };
        format!(
            "CREATE {}INDEX IF NOT EXISTS {} ON {} ({})",
            unique_clause,
            index_name,
            table,
            columns.join(", ")
        )
    }

    /// Generate DROP TABLE statement
    pub fn drop_table(&self, name: &str) -> String {
        format!("DROP TABLE IF EXISTS {}", name)
    }
}

/// Column type enumeration
#[derive(Debug, Clone)]
pub enum ColumnType {
    Text,
    Integer,
    BigInt,
    Real,
    Boolean,
    Json,
    Timestamp,
    Uuid,
}

/// Column definition
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: String,
    pub col_type: ColumnType,
    pub primary_key: bool,
    pub auto_increment: bool,
    pub not_null: bool,
    pub unique: bool,
    pub default: Option<String>,
}

impl ColumnDef {
    pub fn new(name: impl Into<String>, col_type: ColumnType) -> Self {
        Self {
            name: name.into(),
            col_type,
            primary_key: false,
            auto_increment: false,
            not_null: false,
            unique: false,
            default: None,
        }
    }

    pub fn primary_key(mut self) -> Self {
        self.primary_key = true;
        self.not_null = true;
        self
    }

    pub fn auto_increment(mut self) -> Self {
        self.auto_increment = true;
        self.primary_key = true;
        self
    }

    pub fn not_null(mut self) -> Self {
        self.not_null = true;
        self
    }

    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }

    pub fn default_value(mut self, value: impl Into<String>) -> Self {
        self.default = Some(value.into());
        self
    }

    pub fn default_now(mut self) -> Self {
        self.default = Some("CURRENT_TIMESTAMP".to_string());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_table_sqlite() {
        let builder = SchemaBuilder::new(DatabaseDialect::SQLite);
        let columns = vec![
            ColumnDef::new("id", ColumnType::Integer).auto_increment(),
            ColumnDef::new("name", ColumnType::Text).not_null(),
            ColumnDef::new("created_at", ColumnType::Timestamp).default_now(),
        ];

        let sql = builder.create_table("users", &columns);
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS users"));
        assert!(sql.contains("id INTEGER PRIMARY KEY AUTOINCREMENT"));
        assert!(sql.contains("name TEXT NOT NULL"));
    }

    #[test]
    fn test_create_table_postgres() {
        let builder = SchemaBuilder::new(DatabaseDialect::PostgreSQL);
        let columns = vec![
            ColumnDef::new("id", ColumnType::Integer).auto_increment(),
            ColumnDef::new("name", ColumnType::Text).not_null(),
        ];

        let sql = builder.create_table("users", &columns);
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS users"));
        assert!(sql.contains("id SERIAL PRIMARY KEY"));
    }
}
