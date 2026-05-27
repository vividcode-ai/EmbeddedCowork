use std::path::PathBuf;

pub struct OpenCodeConfig {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub binary_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl OpenCodeConfig {
    pub fn new() -> Self {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("embedded-cowork");

        let config_dir = Self::resolve_config_dir();

        Self {
            data_dir: data_dir.clone(),
            config_dir,
            binary_dir: data_dir.join("bin"),
            log_dir: data_dir.join("logs"),
        }
    }

    fn resolve_config_dir() -> PathBuf {
        // 1. Check relative to the executable (dev mode in monorepo):
        //    exe:  packages/server-rust/target/debug/embeddedcowork-server.exe
        //    config: packages/opencode-config
        //    From debug/: ../../../opencode-config
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let candidate = exe_dir.join("..").join("..").join("..").join("opencode-config");
                if candidate.exists() {
                    return candidate;
                }
            }
        }

        // 2. Check relative to CWD (monorepo root when run via npm run rust-dev)
        if let Ok(cwd) = std::env::current_dir() {
            let candidate = cwd.join("packages").join("opencode-config");
            if candidate.exists() {
                return candidate;
            }
        }

        // 3. Check if opencode-config is a sibling of the exe directory (packaged dist)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let candidate = exe_dir.join("opencode-config");
                if candidate.exists() {
                    return candidate;
                }
            }
        }

        // 4. Fall back to user config directory
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("embedded-cowork")
    }
}
