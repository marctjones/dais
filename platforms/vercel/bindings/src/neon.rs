// Neon PostgreSQL database provider for Vercel
//
// This provider uses tokio-postgres to connect to Neon (serverless PostgreSQL)
// which is the recommended database for Vercel Edge Functions.

use async_trait::async_trait;
use dais_core::traits::{DatabaseProvider, DatabaseDialect};
use dais_core::types::{CoreResult, CoreError, Value, Row};
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
    pub async fn new(connection_string: &str) -> CoreResult<Self> {
        let (client, connection) = tokio_postgres::connect(connection_string, NoTls)
            .await
            .map_err(|e| CoreError::DatabaseError(format!("Failed to connect to Neon: {}", e)))?;

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

    /// Convert dais Value to PostgreSQL parameter
    fn value_to_param(value: &Value) -> Box<dyn tokio_postgres::types::ToSql + Send + Sync> {
        match value {
            Value::Null => Box::new(None::<String>),
            Value::Bool(b) => Box::new(*b),
            Value::Integer(i) => Box::new(*i),
            Value::Float(f) => Box::new(*f),
            Value::Text(s) => Box::new(s.clone()),
            Value::Bytes(b) => Box::new(b.clone()),
        }
    }

    /// Convert PostgreSQL row to dais Row
    fn pg_row_to_dais_row(row: &tokio_postgres::Row) -> Row {
        let mut dais_row = Row::new();

        for (idx, column) in row.columns().iter().enumerate() {
            let name = column.name();
            let value = Self::pg_value_to_dais_value(row, idx);
            dais_row.insert(name.to_string(), value);
        }

        dais_row
    }

    /// Convert PostgreSQL value to dais Value
    fn pg_value_to_dais_value(row: &tokio_postgres::Row, idx: usize) -> Value {
        // Try different types in order of likelihood
        if let Ok(v) = row.try_get::<_, Option<String>>(idx) {
            return v.map(Value::Text).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<_, Option<i64>>(idx) {
            return v.map(Value::Integer).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<_, Option<f64>>(idx) {
            return v.map(Value::Float).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<_, Option<bool>>(idx) {
            return v.map(Value::Bool).unwrap_or(Value::Null);
        }
        if let Ok(v) = row.try_get::<_, Option<Vec<u8>>>(idx) {
            return v.map(Value::Bytes).unwrap_or(Value::Null);
        }

        Value::Null
    }
}

#[async_trait]
impl DatabaseProvider for NeonProvider {
    async fn query(&self, sql: &str, params: &[Value]) -> CoreResult<Vec<Row>> {
        // Convert placeholders from SQLite format (?1) to PostgreSQL format ($1)
        let pg_sql = dais_core::sql::convert_placeholders(sql, DatabaseDialect::PostgreSQL);

        let client = self.client.lock().await;

        // Convert parameters
        let pg_params: Vec<Box<dyn tokio_postgres::types::ToSql + Send + Sync>> = params
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
            .map_err(|e| CoreError::DatabaseError(format!("Query failed: {}", e)))?;

        // Convert rows
        Ok(rows.iter().map(Self::pg_row_to_dais_row).collect())
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> CoreResult<u64> {
        // Convert placeholders
        let pg_sql = dais_core::sql::convert_placeholders(sql, DatabaseDialect::PostgreSQL);

        let client = self.client.lock().await;

        // Convert parameters
        let pg_params: Vec<Box<dyn tokio_postgres::types::ToSql + Send + Sync>> = params
            .iter()
            .map(Self::value_to_param)
            .collect();

        // Execute statement
        let rows_affected = client
            .execute(
                &pg_sql,
                &pg_params.iter().map(|p| p.as_ref()).collect::<Vec<_>>()[..],
            )
            .await
            .map_err(|e| CoreError::DatabaseError(format!("Execute failed: {}", e)))?;

        Ok(rows_affected)
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
        let result = provider.query("SELECT 1 as num", &[]).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].get("num"), Some(&Value::Integer(1)));
    }
}
