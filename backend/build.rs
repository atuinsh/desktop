fn main() {
    let channel = std::env::var("APP_CHANNEL").unwrap_or("stable".to_string());
    println!("cargo:rustc-env=APP_CHANNEL={}", channel);
    // Ensure TS-RS bindings are exported to the correct directory
    println!("cargo:rustc-env=TS_RS_EXPORT_DIR=../src/rs-bindings");

    println!("cargo:rerun-if-changed=migrations");
    tauri_build::build()
}
