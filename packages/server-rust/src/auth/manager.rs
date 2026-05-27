
use super::auth_store::{AuthStore, AuthStatus as AuthStoreStatus, SetPasswordParams};
use super::http_auth::{is_loopback_address, parse_cookies};
use super::session_manager::{SessionInfo as SessionManagerSessionInfo, SessionManager};
use super::token_manager::TokenManager;
use crate::logger::Logger;

pub const BOOTSTRAP_TOKEN_STDOUT_PREFIX: &str = "EMBEDDEDCOWORK_BOOTSTRAP_TOKEN:";
pub const DEFAULT_AUTH_USERNAME: &str = "embeddedcowork";
pub const DEFAULT_AUTH_COOKIE_NAME: &str = "embeddedcowork_session";

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub created_at: u64,
    pub username: String,
}

impl From<SessionManagerSessionInfo> for SessionInfo {
    fn from(s: SessionManagerSessionInfo) -> Self {
        SessionInfo {
            id: s.id,
            created_at: s.created_at,
            username: s.username,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthStatus {
    pub username: String,
    pub password_user_provided: bool,
}

impl From<AuthStoreStatus> for AuthStatus {
    fn from(s: AuthStoreStatus) -> Self {
        AuthStatus {
            username: s.username,
            password_user_provided: s.password_user_provided,
        }
    }
}

pub struct AuthManager {
    auth_store: Option<AuthStore>,
    token_manager: Option<TokenManager>,
    session_manager: SessionManager,
    cookie_name: String,
    auth_enabled: bool,
    init_username: String,
    #[allow(dead_code)]
    init_password: Option<String>,
    #[allow(dead_code)]
    generate_token: bool,
    pub config_path: String,
    #[allow(dead_code)]
    logger: Logger,
}

impl AuthManager {
    pub fn new(
        auth_store: AuthStore,
        session_manager: SessionManager,
        token_manager: Option<TokenManager>,
    ) -> Self {
        Self {
            auth_store: Some(auth_store),
            session_manager,
            token_manager,
            cookie_name: DEFAULT_AUTH_COOKIE_NAME.to_string(),
            auth_enabled: true,
            init_username: DEFAULT_AUTH_USERNAME.to_string(),
            init_password: None,
            generate_token: false,
            config_path: String::new(),
            logger: Logger::new("auth"),
        }
    }

    pub fn config_path(&self) -> &str {
        &self.config_path
    }

    pub fn apply_init(&mut self, init: AuthManagerInit) {
        self.config_path = init.config_path;
        if !init.username.is_empty() {
            self.init_username = init.username;
        }
        self.init_password = init.password;
        self.generate_token = init.generate_token;
        self.auth_enabled = !init.dangerously_skip_auth;
        if let Some(name) = init.cookie_name {
            if !name.is_empty() {
                self.cookie_name = name;
            }
        }
    }

    pub fn is_auth_enabled(&self) -> bool {
        self.auth_enabled
    }

    pub fn get_cookie_name(&self) -> &str {
        &self.cookie_name
    }

    pub fn is_token_bootstrap_enabled(&self) -> bool {
        self.token_manager.is_some()
    }

    pub fn issue_bootstrap_token(&mut self) -> Option<String> {
        self.token_manager.as_mut().map(|tm| tm.generate())
    }

    pub fn consume_bootstrap_token(&mut self, token: &str) -> bool {
        self.token_manager.as_mut().map_or(false, |tm| tm.consume(token))
    }

    pub fn validate_login(&mut self, username: &str, password: &str) -> bool {
        if !self.auth_enabled {
            return true;
        }
        self.require_auth_store().validate_credentials(username, password)
    }

    pub fn create_session(&mut self, username: &str) -> SessionInfo {
        if !self.auth_enabled {
            return SessionInfo {
                id: "auth-disabled".to_string(),
                created_at: current_time_millis(),
                username: self.init_username.clone(),
            };
        }
        self.session_manager.create_session(username).into()
    }

    pub fn get_status(&mut self) -> AuthStatus {
        if !self.auth_enabled {
            return AuthStatus {
                username: self.init_username.clone(),
                password_user_provided: false,
            };
        }
        let status = self.require_auth_store().get_status();
        status.unwrap_or(AuthStoreStatus {
            username: self.init_username.clone(),
            password_user_provided: false,
        }).into()
    }

    pub fn set_password(&mut self, password: &str) -> Result<AuthStatus, String> {
        if !self.auth_enabled {
            return Err("Internal authentication is disabled".to_string());
        }
        self.require_auth_store().set_password(&SetPasswordParams {
            password: password.to_string(),
            mark_user_provided: true,
        }).map(|s| s.into())
    }

    pub fn is_loopback_request(&self, remote_addr: Option<&str>) -> bool {
        is_loopback_address(remote_addr)
    }

    pub fn get_session_from_headers(&mut self, headers: &HttpHeaders) -> Option<SessionInfo> {
        if !self.auth_enabled {
            return Some(SessionInfo {
                username: self.init_username.clone(),
                id: "auth-disabled".to_string(),
                created_at: current_time_millis(),
            });
        }

        let cookie_header = headers.cookie.as_deref();
        let cookies = parse_cookies(cookie_header);
        let session_id = cookies.get(&self.cookie_name)?;
        let session = self.session_manager.get_session(Some(session_id))?;
        Some(session.clone().into())
    }

    fn require_auth_store(&mut self) -> &mut AuthStore {
        self.auth_store.as_mut().expect("Auth store is unavailable")
    }
}

pub struct AuthManagerInit {
    pub config_path: String,
    pub username: String,
    pub password: Option<String>,
    pub generate_token: bool,
    pub dangerously_skip_auth: bool,
    pub cookie_name: Option<String>,
}

pub struct HttpHeaders {
    pub cookie: Option<String>,
}

fn current_time_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

