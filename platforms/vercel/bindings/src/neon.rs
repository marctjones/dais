// Neon PostgreSQL database provider for Vercel
//
// This provider uses tokio-postgres to connect to Neon (serverless PostgreSQL)
// which is the recommended database for Vercel Edge Functions.

use async_trait::async_trait;
use dais_core::traits::{DatabaseProvider, DatabaseDialect, PlatformResult, PlatformError, Row, Statement};
use serde_json::Value;
use tokio_postgres::{Client, NoTls};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Neon PostgreSQL provider
///
/// Uses tokio-postgres to connect to Neon serverless PostgreSQL.
/// Connection string format: postgresql://user:pass@host/dbname
pub struct NeonProvider {
    client: Arc<Mutex<Client>>,
}

impl NeonProvider {
    /// Create a new Neon provider with a connection string
    ///
    /// # Example
    ///
    /// ```
    /// use dais_vercel::NeonProvider;
    ///
    /// let connection_string = std::env::var("DATABASE_URL")
    ///     .expect("DATABASE_URL must be set");
    /// let provider = NeonProvider::new(&connection_string).await?;
    /// ```
    pub async fn new(connection_string: &str) -> PlatformResult<Self> {
        let (client, connection) = tokio_postgres::connect(connection_string, NoTls)
            .await
            .map_err(|e| PlatformError::Database(format!("Failed to connect to Neon: {}", e)))?;

        // Spawn connection handler
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("PostgreSQL connection error: {}", e);
            }
        });

        Ok(Self {
            client: Arc::new(Mutex::new(client)),
        })
    }

    /// Convert JSON Value to PostgreSQL parameter
    fn value_to_param(value: &Value) -> Box<dyn tokio_postgres::types::ToSql + Sync> {
        match value {
            Value::Null => Box::new(None::<String>),
            Value::Bool(b) => Box::new(*b),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Box::new(i)
                } else if let Some(f) = n.as_f64() {
                    Box::new(f)
                } else {
                    Box::new(None::<i64>)
                }
            }
            Value::String(s) => Box::new(s.clone()),
            Value::Array(_) | Value::Object(_) => {
                // Serialize complex types as JSON
                Box::new(serde_json::to_string(value).unwrap_or_default())
            }
        }
    }

    /// Convert PostgreSQL row to dais Row
    fn pg_row_to_dais_row(row: &tokio_postgres::Row) -> Row {
        let mut dais_row = Row::new();

        for (idx, column) in row.columns().iter().enumerate() {
            let name = column.name();
            let value = Self::pg_value_to_json_value(row, idx);
            dais_row.insert(name.to_string(), value);
        }

        dais_row
    }

    /// Convert PostgreSQL value to JSON Value
    fn pg_value_to_json_value(row: &tokio_postgres::Row, idx: usize) -> Value {
        // Try different types in order of likelihood
        if let Ok(v) = row.try_get::<_, Option<String>>(idx) {
            return v.map(Value::String).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<_, Option<i64>>(idx) {
            return v.map(|n| Value::Number(n.into())).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<_, Option<f64>>(idx) {
            return v.and_then(|f| serde_json::Number::from_f64(f))
                .map(Value::Number)
                .unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<_, Option<bool>>(idx) {
            return v.map(Value::Bool).unwrap_or(Value::Null);
        }

        Value::Null
    }
}

#[async_trait(?Send)]
impl DatabaseProvider for NeonProvider {
    async fn execute(&self, sql: &str, params: &[Value]) -> PlatformResult<Vec<Row>> {
        // Convert placeholders from SQLite format (?1) to PostgreSQL format ($1)
        let pg_sql = dais_core::sql::convert_placeholders(sql, DatabaseDialect::PostgreSQL);

        let client = self.client.lock().await;

        // Convert parameters
        let pg_params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync>> = params
            .iter()
            .map(Self::value_to_param)
            .collect();

        // Execute query
        let rows = client
            .query(
                &pg_sql,
                &pg_params.iter().map(|p| p.as_ref()).collect::<Vec<_>>()[..],
            )
            .await
            .map_err(|e| PlatformError::Database(format!("Query failed: {}", e)))?;

        // Convert rows
        Ok(rows.iter().map(Self::pg_row_to_dais_row).collect())
    }

    async fn batch(&self, statements: Vec<Statement>) -> PlatformResult<()> {
        let mut client = self.client.lock().await;

        // Start transaction
        let transaction = client
            .transaction()
            .await
            .map_err(|e| PlatformError::Database(format!("Failed to start transaction: {}", e)))?;

        for statement in statements {
            // Convert placeholders
            let pg_sql = dais_core::sql::convert_placeholders(&statement.sql, DatabaseDialect::PostgreSQL);

            // Convert parameters
            let pg_params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync>> = statement
                .params
                .iter()
                .map(Self::value_to_param)
                .collect();

            // Execute statement
            transaction
                .execute(
                    &pg_sql,
                    &pg_params.iter().map(|p| p.as_ref()).collect::<Vec<_>>()[..],
                )
                .await
                .map_err(|e| PlatformError::Database(format!("Batch statement failed: {}", e)))?;
        }

        // Commit transaction
        transaction
            .commit()
            .await
            .map_err(|e| PlatformError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    fn dialect(&self) -> DatabaseDialect {
        DatabaseDialect::PostgreSQL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires actual Neon database
    async fn test_neon_connection() {
        let connection_string = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set for tests");

        let provider = NeonProvider::new(&connection_string).await.unwrap();

        // Test simple query
        let result = provider.execute("SELECT 1 as num", &[]).await.unwrap();
        assert_eq!(result.len(), 1);
    }
}
