/// D1 database provider implementation for Cloudflare Workers
///
/// Implements the DatabaseProvider trait using Cloudflare D1 (SQLite at the edge)

use dais_core::traits::{DatabaseProvider, DatabaseDialect, PlatformError, PlatformResult, Row, Statement};
use async_trait::async_trait;
use serde_json::Value;
use worker::D1Database;

pub struct D1Provider {
    db: D1Database,
}

impl D1Provider {
    /// Create a new D1Provider from a Cloudflare D1 binding
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

#[async_trait(?Send)]
impl DatabaseProvider for D1Provider {
    async fn execute(&self, sql: &str, params: &[Value]) -> PlatformResult<Vec<Row>> {
        // For now, simplified implementation without parameter binding
        // TODO: Implement proper parameter binding when worker-rs API is clearer

        let statement = self.db.prepare(sql);

        // Execute and get results
        let result = statement
            .all()
            .await
            .map_err(|e| PlatformError::Database(format!("D1 query failed: {:?}", e)))?;

        // Convert D1 results to our Row type
        let results = result
            .results::<serde_json::Map<String, Value>>()
            .map_err(|e| PlatformError::Database(format!("Failed to parse D1 results: {:?}", e)))?;

        let rows = results
            .into_iter()
            .map(|map| {
                let mut row = Row::new();
                for (key, value) in map {
                    row.insert(key, value);
                }
                row
            })
            .collect();

        Ok(rows)
    }

    async fn batch(&self, statements: Vec<Statement>) -> PlatformResult<()> {
        // D1 batch API (simplified without parameter binding)
        // TODO: Add parameter binding support
        let batch_statements: Vec<_> = statements
            .iter()
            .map(|stmt| self.db.prepare(&stmt.sql))
            .collect();

        self.db
            .batch(batch_statements)
            .await
            .map_err(|e| PlatformError::Database(format!("D1 batch failed: {:?}", e)))?;

        Ok(())
    }

    fn dialect(&self) -> DatabaseDialect {
        DatabaseDialect::SQLite
    }
}

/// Helper to convert worker::Error to PlatformError
fn worker_error_to_platform(err: worker::Error) -> PlatformError {
    PlatformError::Database(format!("Worker error: {:?}", err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialect() {
        // Can't test with real D1 in unit tests, but we can test dialect
        // In integration tests with wrangler dev, we can test real queries
    }
}
