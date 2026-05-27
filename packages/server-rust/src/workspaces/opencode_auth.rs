use rand::Rng;

pub const OPENCODE_SERVER_USERNAME_ENV: &str = "OPENCODE_SERVER_USERNAME";
pub const OPENCODE_SERVER_PASSWORD_ENV: &str = "OPENCODE_SERVER_PASSWORD";
pub const DEFAULT_OPENCODE_USERNAME: &str = "embeddedcowork";

pub fn generate_opencode_server_password() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill(&mut bytes);
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn build_opencode_basic_auth_header(username: &str, password: &str) -> Option<String> {
    if username.is_empty() || password.is_empty() {
        return None;
    }
    let token = base64_encode(&format!("{}:{}", username, password));
    Some(format!("Basic {}", token))
}

fn base64_encode(input: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(input)
}
