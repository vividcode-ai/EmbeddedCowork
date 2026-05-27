use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use axum::body::Body;
use axum::response::Response;
use http::StatusCode;

use crate::logger::Logger;
use crate::server::proxy::SharedProxyClient;

#[derive(Clone, Debug)]
pub struct RemoteProxySession {
    pub id: String,
    pub target_url: String,
    pub proxy_url: String,
    pub allowed_origins: Vec<String>,
    pub created_at: String,
}

pub struct RemoteProxyManager {
    sessions: Arc<Mutex<HashMap<String, RemoteProxySession>>>,
    #[allow(dead_code)]
    logger: Logger,
    proxy_client: SharedProxyClient,
}

impl RemoteProxyManager {
    pub fn new(logger: Logger, proxy_client: SharedProxyClient) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            logger,
            proxy_client,
        }
    }

    pub async fn list(&self) -> Vec<RemoteProxySession> {
        self.sessions.lock().await.values().cloned().collect()
    }

    pub async fn get(&self, id: &str) -> Option<RemoteProxySession> {
        self.sessions.lock().await.get(id).cloned()
    }

    pub async fn create(&self, target_url: &str, allowed_origins: Vec<String>) -> RemoteProxySession {
        let id = uuid::Uuid::new_v4().to_string();
        let session = RemoteProxySession {
            id: id.clone(),
            target_url: target_url.to_string(),
            proxy_url: format!("/proxy/{}", id),
            allowed_origins,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.sessions.lock().await.insert(id, session.clone());
        session
    }

    pub async fn delete(&self, id: &str) {
        self.sessions.lock().await.remove(id);
    }

    pub async fn shutdown(&self) {
        self.sessions.lock().await.clear();
    }

    pub async fn forward(
        &self,
        session_id: &str,
        method: reqwest::Method,
        path: &str,
        headers: &http::HeaderMap,
        body: Vec<u8>,
    ) -> Result<Response<Body>, StatusCode> {
        let session = self
            .sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or(StatusCode::NOT_FOUND)?;

        let base = session.target_url.trim_end_matches('/');
        let clean_path = path.trim_start_matches('/');
        let target_url = format!("{}/{}", base, clean_path);

        self.proxy_client
            .forward_request(method, &target_url, headers, body)
            .await
    }
}
