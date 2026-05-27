use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::logger::Logger;

const STALE_CONNECTION_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Debug, Clone)]
pub struct ClientConnectionRef {
    pub client_id: String,
    pub connection_id: String,
}

#[derive(Debug, Clone)]
pub struct ClientConnectionRecord {
    pub client_id: String,
    pub connection_id: String,
    pub key: String,
    pub connected_at: Instant,
    pub last_seen_at: Instant,
}

#[derive(Debug, Clone)]
pub enum ConnectionChangeEvent {
    Connected { connection: ClientConnectionRecord },
    Disconnected { connection: ClientConnectionRecord, reason: String },
}

struct RegisteredConnection {
    record: ClientConnectionRecord,
    close: Option<Box<dyn FnOnce() + Send>>,
}

pub struct ClientConnectionManager {
    connections: HashMap<String, RegisteredConnection>,
    subscribers: Vec<Box<dyn Fn(&ConnectionChangeEvent) + Send + Sync>>,
}

impl ClientConnectionManager {
    pub fn new(_logger: Logger) -> Self {
        Self {
            connections: HashMap::new(),
            subscribers: Vec::new(),
        }
    }

    pub fn subscribe<F>(&mut self, listener: F)
    where
        F: Fn(&ConnectionChangeEvent) + Send + Sync + 'static,
    {
        self.subscribers.push(Box::new(listener));
    }

    pub fn register<F>(&mut self, input: ClientConnectionRef, close: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let key = get_connection_key(&input);
        let now = Instant::now();

        if self.connections.contains_key(&key) {
            self.disconnect(&key, "replaced");
        }

        let record = ClientConnectionRecord {
            client_id: input.client_id.clone(),
            connection_id: input.connection_id.clone(),
            key: key.clone(),
            connected_at: now,
            last_seen_at: now,
        };

        self.connections.insert(key, RegisteredConnection {
            record: record.clone(),
            close: Some(Box::new(close)),
        });

        self.notify(&ConnectionChangeEvent::Connected { connection: record });
    }

    pub fn pong(&mut self, input: &ClientConnectionRef) -> bool {
        let key = get_connection_key(input);
        match self.connections.get_mut(&key) {
            Some(conn) => {
                conn.record.last_seen_at = Instant::now();
                true
            }
            None => false,
        }
    }

    pub fn is_connected(&self, input: &ClientConnectionRef) -> bool {
        let key = get_connection_key(input);
        self.connections.contains_key(&key)
    }

    pub fn sweep_stale(&mut self) {
        let cutoff = Instant::now() - STALE_CONNECTION_TIMEOUT;
        let stale_keys: Vec<String> = self
            .connections
            .iter()
            .filter(|(_, conn)| conn.record.last_seen_at < cutoff)
            .map(|(key, _)| key.clone())
            .collect();

        for key in stale_keys {
            self.disconnect(&key, "timeout");
        }
    }

    fn disconnect(&mut self, key: &str, reason: &str) {
        if let Some(mut conn) = self.connections.remove(key) {
            if let Some(close) = conn.close.take() {
                close();
            }
            self.notify(&ConnectionChangeEvent::Disconnected {
                connection: conn.record.clone(),
                reason: reason.to_string(),
            });
        }
    }

    fn notify(&self, event: &ConnectionChangeEvent) {
        for subscriber in &self.subscribers {
            subscriber(event);
        }
    }
}

fn get_connection_key(input: &ClientConnectionRef) -> String {
    format!("{}:{}", input.client_id, input.connection_id)
}
