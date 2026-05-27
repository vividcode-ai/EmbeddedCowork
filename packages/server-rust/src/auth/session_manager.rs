use std::collections::HashMap;
use rand::Rng;

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub created_at: u64,
    pub username: String,
}

pub struct SessionManager {
    sessions: HashMap<String, SessionInfo>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn create_session(&mut self, username: &str) -> SessionInfo {
        let id = generate_session_id();
        let info = SessionInfo {
            id: id.clone(),
            created_at: current_time_millis(),
            username: username.to_string(),
        };
        self.sessions.insert(id, info.clone());
        info
    }

    pub fn get_session(&self, id: Option<&str>) -> Option<&SessionInfo> {
        id.and_then(|id| self.sessions.get(id))
    }
}

fn generate_session_id() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill(&mut bytes);
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn current_time_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
