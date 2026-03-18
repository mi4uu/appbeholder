use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

#[derive(Debug, Clone)]
pub struct LogEvent {
    pub html: String,
}

const CHANNEL_CAPACITY: usize = 256;

#[derive(Clone)]
pub struct SseChannels {
    log_channels: Arc<RwLock<HashMap<String, broadcast::Sender<LogEvent>>>>,
}

impl SseChannels {
    pub fn new() -> Self {
        Self {
            log_channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn publish_log(&self, project_slug: &str, event: LogEvent) {
        let channels = self.log_channels.read().await;
        if let Some(tx) = channels.get(project_slug) {
            // Ignore send errors (no subscribers)
            let _ = tx.send(event);
        }
    }

    pub async fn subscribe_logs(&self, project_slug: &str) -> broadcast::Receiver<LogEvent> {
        // Check if channel exists
        {
            let channels = self.log_channels.read().await;
            if let Some(tx) = channels.get(project_slug) {
                return tx.subscribe();
            }
        }

        // Create new channel
        let mut channels = self.log_channels.write().await;
        // Double-check after acquiring write lock
        if let Some(tx) = channels.get(project_slug) {
            return tx.subscribe();
        }

        let (tx, rx) = broadcast::channel(CHANNEL_CAPACITY);
        channels.insert(project_slug.to_string(), tx);
        rx
    }
}
