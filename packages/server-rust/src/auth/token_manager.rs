use rand::Rng;

#[derive(Debug, Clone)]
pub struct BootstrapToken {
    pub token: String,
    pub created_at: u64,
    pub consumed: bool,
}

pub struct TokenManager {
    token: Option<BootstrapToken>,
    ttl_ms: u64,
}

impl TokenManager {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            token: None,
            ttl_ms,
        }
    }

    pub fn generate(&mut self) -> String {
        let token = generate_random_token();
        self.token = Some(BootstrapToken {
            token: token.clone(),
            created_at: current_time_millis(),
            consumed: false,
        });
        token
    }

    pub fn consume(&mut self, token: &str) -> bool {
        match &self.token {
            Some(t) => {
                if t.consumed {
                    return false;
                }
                if current_time_millis() - t.created_at > self.ttl_ms {
                    return false;
                }
                if token != t.token {
                    return false;
                }
                self.token.as_mut().unwrap().consumed = true;
                true
            }
            None => false,
        }
    }

    pub fn peek(&self) -> Option<&str> {
        self.token.as_ref().map(|t| t.token.as_str())
    }
}

fn generate_random_token() -> String {
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
