use std::path::PathBuf;

pub fn get_opencode_binary_dir() -> PathBuf {
    let data_dir = dirs_data_dir();
    data_dir.join("bin")
}

pub fn get_opencode_binary_path(platform: &str) -> PathBuf {
    let bin_dir = get_opencode_binary_dir();
    let binary_name = if platform == "win32" {
        "opencode.exe"
    } else {
        "opencode"
    };
    bin_dir.join(binary_name)
}

pub fn get_opencode_download_url(version: &str, platform: &str, arch: &str) -> String {
    let ext = if platform == "win32" { "zip" } else { "tar.gz" };
    format!(
        "https://github.com/vividcode-ai/opencode/releases/download/v{}/opencode-v{}-{}-{}.{}",
        version, version, platform, arch, ext
    )
}

fn dirs_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("embedded-cowork")
}
