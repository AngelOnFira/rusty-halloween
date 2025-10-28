/// Logging utilities with automatic file:line injection
///
/// This module provides wrapper macros around the log crate that automatically
/// inject file and line number information for easy navigation.
///
/// Format: [file:line] message
///
/// Example:
/// ```
/// info!("state::ota: Starting OTA update");
/// // Output: [src/state/ota.rs:29] state::ota: Starting OTA update
/// ```

/// Info-level log with automatic file:line prefix
/// Format: [file:line] message
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        {
            const LOC: &str = concat!("[", file!(), ":", line!(), "]");
            ::log::info!("{} {}", LOC, format_args!($($arg)*))
        }
    };
}

/// Warning-level log with automatic file:line prefix
/// Format: [file:line] message
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        {
            const LOC: &str = concat!("[", file!(), ":", line!(), "]");
            ::log::warn!("{} {}", LOC, format_args!($($arg)*))
        }
    };
}

/// Error-level log with automatic file:line prefix
/// Format: [file:line] message
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        {
            const LOC: &str = concat!("[", file!(), ":", line!(), "]");
            ::log::error!("{} {}", LOC, format_args!($($arg)*))
        }
    };
}

/// Debug-level log with automatic file:line prefix
/// Format: [file:line] message
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        {
            const LOC: &str = concat!("[", file!(), ":", line!(), "]");
            ::log::debug!("{} {}", LOC, format_args!($($arg)*))
        }
    };
}

/// Trace-level log with automatic file:line prefix
/// Format: [file:line] message
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        {
            const LOC: &str = concat!("[", file!(), ":", line!(), "]");
            ::log::trace!("{} {}", LOC, format_args!($($arg)*))
        }
    };
}

