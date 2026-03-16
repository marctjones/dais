/// Query builder for portable SQL queries
///
/// Provides a simple API to build SQL queries that work across dialects

use crate::traits::DatabaseDialect;
use super::convert_placeholders;

pub struct QueryBuilder {
    dialect: DatabaseDialect,
    query: String,
}

impl QueryBuilder {
    /// Create a new QueryBuilder for the given dialect
    pub fn new(dialect: DatabaseDialect) -> Self {
        Self {
            dialect,
            query: String::new(),
        }
    }

    /// Add raw SQL to the query
    pub fn raw(mut self, sql: &str) -> Self {
        self.query.push_str(sql);
        self
    }

    /// Add a SELECT clause
    pub fn select(mut self, columns: &[&str]) -> Self {
        self.query.push_str("SELECT ");
        self.query.push_str(&columns.join(", "));
        self
    }

    /// Add a FROM clause
    pub fn from(mut self, table: &str) -> Self {
        self.query.push_str(" FROM ");
        self.query.push_str(table);
        self
    }

    /// Add a WHERE clause
    pub fn where_clause(mut self, condition: &str) -> Self {
        self.query.push_str(" WHERE ");
        self.query.push_str(condition);
        self
    }

    /// Add an AND condition
    pub fn and(mut self, condition: &str) -> Self {
        self.query.push_str(" AND ");
        self.query.push_str(condition);
        self
    }

    /// Add an OR condition
    pub fn or(mut self, condition: &str) -> Self {
        self.query.push_str(" OR ");
        self.query.push_str(condition);
        self
    }

    /// Add an ORDER BY clause
    pub fn order_by(mut self, columns: &[&str]) -> Self {
        self.query.push_str(" ORDER BY ");
        self.query.push_str(&columns.join(", "));
        self
    }

    /// Add a LIMIT clause
    pub fn limit(mut self, limit: u32) -> Self {
        self.query.push_str(&format!(" LIMIT {}", limit));
        self
    }

    /// Add an OFFSET clause
    pub fn offset(mut self, offset: u32) -> Self {
        self.query.push_str(&format!(" OFFSET {}", offset));
        self
    }

    /// Build the final query string with proper parameter placeholders
    pub fn build(self) -> String {
        convert_placeholders(&self.query, self.dialect)
    }
}

/// Helper macro for building queries
#[macro_export]
macro_rules! query {
    ($dialect:expr, $sql:expr) => {
        $crate::sql::convert_placeholders($sql, $dialect)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_builder() {
        let query = QueryBuilder::new(DatabaseDialect::SQLite)
            .select(&["id", "name"])
            .from("users")
            .where_clause("id = ?1")
            .order_by(&["name"])
            .limit(10)
            .build();

        assert_eq!(query, "SELECT id, name FROM users WHERE id = ?1 ORDER BY name LIMIT 10");
    }

    #[test]
    fn test_query_builder_postgres() {
        let query = QueryBuilder::new(DatabaseDialect::PostgreSQL)
            .select(&["id", "name"])
            .from("users")
            .where_clause("id = ?1")
            .build();

        assert_eq!(query, "SELECT id, name FROM users WHERE id = $1");
    }
}
