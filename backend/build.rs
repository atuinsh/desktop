fn main() {
    let channel = std::env::var("APP_CHANNEL").unwrap_or("stable".to_string());
    println!("cargo:rustc-env=APP_CHANNEL={}", channel);

    println!("cargo:rerun-if-changed=migrations");
    tauri_build::build()
}
