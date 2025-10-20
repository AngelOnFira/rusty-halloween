fn main() {
    // Print the current date and time
    println!("Current date and time: {}", chrono::Utc::now().to_string());

    // Print the current working directory
    println!("Current working directory: {}", std::env::current_dir().unwrap().to_string_lossy());

    // Print the user's home directory
    println!("User's home directory: {}", std::env::var("HOME").unwrap());

    println!("Hello, world!");
}
