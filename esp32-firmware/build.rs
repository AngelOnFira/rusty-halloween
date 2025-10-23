fn main() {
    // ESP-IDF build configuration
    embuild::espidf::sysenv::output();

    // Set build timestamp for version tracking
    use std::process::Command;
    let timestamp = Command::new("date")
        .arg("+%Y-%m-%d %H:%M:%S UTC")
        .env("TZ", "UTC")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", timestamp);

    // Re-run if this file changes
    println!("cargo:rerun-if-changed=build.rs");
}
