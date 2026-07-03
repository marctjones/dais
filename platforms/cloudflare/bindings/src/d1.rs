use async_trait::async_trait;
/// D1 database provider implementation for Cloudflare Workers
///
/// Implements the DatabaseProvider trait using Cloudflare D1 (SQLite at the edge)
use dais_core::traits::{
    DatabaseDialect, DatabaseProvider, PlatformError, PlatformResult, Row, Statement,
};
use serde_json::Value;
use wasm_bindgen::JsValue;
use worker::D1Database;

/// Convert a JSON value into a JsValue suitable for D1 parameter binding.
/// The core always binds scalars (string / number / bool / null); arrays and
/// objects are JSON-stringified (they correspond to TEXT/JSON columns).
fn json_to_js(value: &Value) -> JsValue {
    match value {
        Value::Null => JsValue::NULL,
        Value::Bool(b) => JsValue::from_bool(*b),
        Value::Number(n) => n.as_f64().map(JsValue::from_f64).unwrap_or(JsValue::NULL),
        Value::String(s) => JsValue::from_str(s),
        other => JsValue::from_str(&other.to_string()),
    }
}

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
        let statement = self.db.prepare(sql);

        // Bind positional parameters (?1, ?2, …). Without this, any query with
        // placeholders fails with "Wrong number of parameter bindings".
        let statement = if params.is_empty() {
            statement
        } else {
            let bound: Vec<JsValue> = params.iter().map(json_to_js).collect();
            statement
                .bind(&bound)
                .map_err(|e| PlatformError::Database(format!("D1 bind failed: {:?}", e)))?
        };

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
        let mut prepared = Vec::with_capacity(statements.len());
        for stmt in &statements {
            let p = self.db.prepare(&stmt.sql);
            let p = if stmt.params.is_empty() {
                p
            } else {
                let bound: Vec<JsValue> = stmt.params.iter().map(json_to_js).collect();
                p.bind(&bound)
                    .map_err(|e| PlatformError::Database(format!("D1 bind failed: {:?}", e)))?
            };
            prepared.push(p);
        }

        self.db
            .batch(prepared)
            .await
            .map_err(|e| PlatformError::Database(format!("D1 batch failed: {:?}", e)))?;

        Ok(())
    }

    fn dialect(&self) -> DatabaseDialect {
        DatabaseDialect::SQLite
    }
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
