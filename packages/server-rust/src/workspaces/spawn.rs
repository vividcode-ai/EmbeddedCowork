use std::collections::HashMap;

pub struct SpawnSpec {
    pub command: String,
    pub args: Vec<String>,
    pub current_dir: Option<String>,
    pub env: Option<HashMap<String, String>>,
}

pub fn build_spawn_spec(
    binary_path: &str,
    args: &[String],
    cwd: &str,
    env: &HashMap<String, String>,
) -> SpawnSpec {
    SpawnSpec {
        command: binary_path.to_string(),
        args: args.to_vec(),
        current_dir: Some(cwd.to_string()),
        env: Some(env.clone()),
    }
}

pub fn build_wsl_signal_spec(_distro: &str, _linux_pid: u32, _signal: &str) -> SpawnSpec {
    SpawnSpec {
        command: "wsl".to_string(),
        args: vec!["--kill".to_string()],
        current_dir: None,
        env: None,
    }
}
