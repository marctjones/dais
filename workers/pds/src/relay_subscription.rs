///! Durable Object for handling relay WebSocket subscriptions

use serde::{Deserialize, Serialize};
use serde_json::json;
use worker::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEvent {
    pub repo: String,
    pub operation: String,
    pub path: String,
    pub cid: String,
    pub record: serde_json::Value,
}

#[durable_object]
pub struct RelaySubscription {
    state: State,
    env: Env,
}

impl DurableObject for RelaySubscription {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        let url = req.url()?;
        let path = url.path();
        let path_str = path.to_string();

        // Check if this is a WebSocket upgrade request
        let is_websocket = req.headers()
            .get("Upgrade")
            .ok()
            .flatten()
            .map(|v| v.to_lowercase() == "websocket")
            .unwrap_or(false);

        match path_str.as_str() {
            "/broadcast" => self.handle_broadcast(req).await,
            _ if is_websocket => self.handle_subscribe(req).await,
            _ => {
                // Return info about the firehose endpoint
                let info = json!({
                    "message": "WebSocket endpoint for com.atproto.sync.subscribeRepos",
                    "usage": "Connect with WebSocket client to receive repo events"
                });
                Response::from_json(&info)
            }
        }
    }
}

impl RelaySubscription {
    /// Handle WebSocket subscription from relay
    async fn handle_subscribe(&self, req: Request) -> Result<Response> {
        // Get query parameters for cursor (optional)
        let url = req.url()?;
        let cursor: Option<i64> = url
            .query_pairs()
            .find(|(k, _)| k == "cursor")
            .and_then(|(_, v)| v.parse().ok());

        // Upgrade to WebSocket
        let pair = WebSocketPair::new()?;
        let client = pair.client;
        let server = pair.server;

        // Accept the WebSocket
        server.accept()?;

        // Get stored events since cursor
        if let Some(cursor_seq) = cursor {
            // Send historical events
            if let Some(events_json) = self.state.storage().get::<String>("events").await.ok().flatten() {
                if let Ok(events) = serde_json::from_str::<Vec<(i64, RepoEvent)>>(&events_json) {
                    for (seq, event) in events {
                        if seq > cursor_seq {
                            let frame = json!({
                                "op": 1,
                                "t": "#commit",
                                "seq": seq,
                                "rebase": false,
                                "tooBig": false,
                                "repo": event.repo,
                                "commit": event.cid,
                                "rev": format!("{}", seq),
                                "since": null,
                                "blocks": [],
                                "ops": [{
                                    "action": event.operation,
                                    "path": event.path,
                                    "cid": event.cid
                                }],
                                "blobs": [],
                                "time": chrono::Utc::now().to_rfc3339()
                            });

                            server.send_with_str(&serde_json::to_string(&frame)?)?;
                        }
                    }
                }
            }
        }

        // Send info message
        let info = json!({
            "op": 1,
            "t": "#info",
            "name": "dais-pds",
            "message": "Connected to dais PDS firehose"
        });
        server.send_with_str(&serde_json::to_string(&info)?)?;

        // Return the client WebSocket to complete the handshake
        Response::from_websocket(client)
    }

    /// Broadcast event to all connected WebSocket subscribers
    async fn handle_broadcast(&self, mut req: Request) -> Result<Response> {
        let event: RepoEvent = req.json().await?;

        // Get current sequence number
        let seq = self.state.storage().get::<i64>("seq").await.ok().flatten().unwrap_or(0) + 1;
        self.state.storage().put("seq", seq).await?;

        // Store event for historical replay
        let mut events: Vec<(i64, RepoEvent)> = self
            .state
            .storage()
            .get::<String>("events")
            .await
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        events.push((seq, event.clone()));

        // Keep only last 1000 events
        if events.len() > 1000 {
            events.drain(0..events.len() - 1000);
        }

        self.state
            .storage()
            .put("events", serde_json::to_string(&events)?)
            .await?;

        // Broadcast to all connected WebSockets
        let frame = json!({
            "op": 1,
            "t": "#commit",
            "seq": seq,
            "rebase": false,
            "tooBig": false,
            "repo": event.repo,
            "commit": event.cid,
            "rev": format!("{}", seq),
            "since": null,
            "blocks": [],
            "ops": [{
                "action": event.operation,
                "path": event.path,
                "cid": event.cid
            }],
            "blobs": [],
            "time": chrono::Utc::now().to_rfc3339()
        });

        let message = serde_json::to_string(&frame)?;

        // Get all WebSocket connections and broadcast
        let connections = self.state.get_websockets();
        for ws in connections {
            if let Err(e) = ws.send_with_str(&message) {
                console_log!("Failed to send to WebSocket: {:?}", e);
            }
        }

        Response::ok("Event broadcasted")
    }
}
