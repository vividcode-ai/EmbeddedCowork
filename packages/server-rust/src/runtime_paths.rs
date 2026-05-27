use std::path::PathBuf;

pub fn get_package_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Navigate up to find the workspace root
    if dir.ends_with("packages") {
        dir.pop();
    } else if dir.ends_with("server-rust") {
        dir.pop();
        dir.pop();
    }
    dir
}

pub fn get_ui_dist_dir() -> PathBuf {
    get_package_root().join("packages").join("ui").join("dist")
}

pub fn get_ui_public_dir() -> PathBuf {
    get_package_root().join("packages").join("server").join("public")
}
