use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::auth::password_hash::{hash_password, verify_password, PasswordHashRecord};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthFile {
    pub version: u8,
    pub username: String,
    pub password: PasswordHashRecord,
    pub user_provided: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    pub username: String,
    pub password_user_provided: bool,
}

pub struct AuthStore {
    auth_file_path: PathBuf,
    cached_file: Option<AuthFile>,
    override_auth: Option<AuthFile>,
    bootstrap_username: Option<String>,
}

impl AuthStore {
    pub fn new(auth_file_path: PathBuf) -> Self {
        Self {
            auth_file_path,
            cached_file: None,
            override_auth: None,
            bootstrap_username: None,
        }
    }

    pub fn get_auth_file_path(&self) -> &Path {
        &self.auth_file_path
    }

    pub fn load(&mut self) -> Option<&AuthFile> {
        if let Some(ref override_auth) = self.override_auth {
            return Some(override_auth);
        }

        if self.cached_file.is_some() {
            return self.cached_file.as_ref();
        }

        if !self.auth_file_path.exists() {
            return None;
        }

        match fs::read_to_string(&self.auth_file_path) {
            Ok(raw) => {
                match serde_json::from_str::<AuthFile>(&raw) {
                    Ok(parsed) => {
                        if parsed.version != 1 {
                            return None;
                        }
                        self.cached_file = Some(parsed);
                        self.cached_file.as_ref()
                    }
                    Err(_) => None,
                }
            }
            Err(_) => None,
        }
    }

    pub fn ensure_initialized(&mut self, params: &AuthInitParams) -> Result<(), String> {
        let password = params.password.as_deref().and_then(|p| {
            let trimmed = p.trim();
            if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
        });

        if let Some(ref pw) = password {
            let now = chrono::Utc::now().to_rfc3339();
            let runtime = AuthFile {
                version: 1,
                username: params.username.clone(),
                password: hash_password(pw),
                user_provided: true,
                updated_at: now,
            };
            self.override_auth = Some(runtime);
            self.cached_file = None;
            self.bootstrap_username = None;
            return Ok(());
        }

        if self.load().is_some() {
            self.bootstrap_username = None;
            return Ok(());
        }

        if params.allow_bootstrap_without_password {
            self.bootstrap_username = Some(params.username.clone());
            return Ok(());
        }

        Err(format!(
            "No server password configured. Create {} or start with --password / EMBEDDEDCOWORK_SERVER_PASSWORD.",
            self.auth_file_path.display()
        ))
    }

    pub fn validate_credentials(&mut self, username: &str, password: &str) -> bool {
        let auth = match self.load() {
            Some(a) => a.clone(),
            None => return false,
        };

        if username != auth.username {
            return false;
        }

        verify_password(password, &auth.password)
    }

    pub fn set_password(&mut self, params: &SetPasswordParams) -> Result<AuthStatus, String> {
        if self.override_auth.is_some() {
            return Err(
                "Server password is provided via CLI/env and cannot be changed while running. \
                 Restart without --password / EMBEDDEDCOWORK_SERVER_PASSWORD to use auth.json."
                    .to_string(),
            );
        }

        let current = self.load().cloned();

        match current {
            None => {
                let username = self.bootstrap_username.as_ref().ok_or("Auth is not initialized")?;
                let created = AuthFile {
                    version: 1,
                    username: username.clone(),
                    password: hash_password(&params.password),
                    user_provided: params.mark_user_provided,
                    updated_at: chrono::Utc::now().to_rfc3339(),
                };
                self.persist(&created)?;
                self.bootstrap_username = None;
                Ok(AuthStatus {
                    username: created.username,
                    password_user_provided: created.user_provided,
                })
            }
            Some(mut current) => {
                current.password = hash_password(&params.password);
                current.user_provided = params.mark_user_provided;
                current.updated_at = chrono::Utc::now().to_rfc3339();
                self.persist(&current)?;
                Ok(AuthStatus {
                    username: current.username,
                    password_user_provided: current.user_provided,
                })
            }
        }
    }

    pub fn get_status(&mut self) -> Result<AuthStatus, String> {
        if let Some(current) = self.load() {
            return Ok(AuthStatus {
                username: current.username.clone(),
                password_user_provided: current.user_provided,
            });
        }

        if let Some(ref username) = self.bootstrap_username {
            return Ok(AuthStatus {
                username: username.clone(),
                password_user_provided: false,
            });
        }

        Err("Auth is not initialized".to_string())
    }

    fn persist(&mut self, auth: &AuthFile) -> Result<(), String> {
        if let Some(parent) = self.auth_file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(auth).map_err(|e| e.to_string())?;
        fs::write(&self.auth_file_path, &json).map_err(|e| e.to_string())?;
        self.cached_file = Some(auth.clone());
        Ok(())
    }
}

pub struct AuthInitParams {
    pub username: String,
    pub password: Option<String>,
    pub allow_bootstrap_without_password: bool,
}

pub struct SetPasswordParams {
    pub password: String,
    pub mark_user_provided: bool,
}
