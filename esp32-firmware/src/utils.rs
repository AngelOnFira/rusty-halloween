// Include the entire .env file as a string at compile time
const ENV_FILE: &str = include_str!("../.env");

/// Simple function to extract a value from the .env content
pub fn get_embedded_env_value(key: &str) -> String {
    let search_pattern = format!("{}=", key);

    for line in ENV_FILE.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        if let Some(value) = line.strip_prefix(&search_pattern) {
            // Remove surrounding quotes if present and return
            return value.trim_matches('"').trim_matches('\'').to_string();
        }
    }

    panic!("Environment variable '{}' not found in .env file. Make sure your .env file contains a line like: {}=your_value", key, key);
}

/// Get a human-readable string for WiFi disconnect reasons
pub fn get_disconnect_reason_string(reason: u8) -> &'static str {
    match reason {
        2 => "AUTH_EXPIRE",
        3 => "AUTH_LEAVE",
        4 => "ASSOC_EXPIRE",
        5 => "ASSOC_TOOMANY",
        6 => "NOT_AUTHED",
        7 => "NOT_ASSOCED",
        8 => "ASSOC_LEAVE",
        9 => "ASSOC_NOT_AUTHED",
        10 => "DISASSOC_PWRCAP_BAD",
        11 => "DISASSOC_SUPCHAN_BAD",
        13 => "IE_INVALID",
        14 => "MIC_FAILURE",
        15 => "4WAY_HANDSHAKE_TIMEOUT",
        16 => "GROUP_KEY_UPDATE_TIMEOUT",
        17 => "IE_IN_4WAY_DIFFERS",
        18 => "GROUP_CIPHER_INVALID",
        19 => "PAIRWISE_CIPHER_INVALID",
        20 => "AKMP_INVALID",
        21 => "UNSUPP_RSN_IE_VERSION",
        22 => "INVALID_RSN_IE_CAP",
        23 => "802_1X_AUTH_FAILED",
        24 => "CIPHER_SUITE_REJECTED",
        200 => "BEACON_TIMEOUT",
        201 => "NO_AP_FOUND",
        202 => "AUTH_FAIL",
        203 => "ASSOC_FAIL",
        204 => "HANDSHAKE_TIMEOUT",
        205 => "CONNECTION_FAIL",
        206 => "AP_TSF_RESET",
        207 => "ROAMING",
        208 => "ASSOC_COMEBACK_TIME_TOO_LONG",
        209 => "SA_QUERY_TIMEOUT",
        210 => "NO_AP_FOUND_W_COMPATIBLE_SECURITY",
        211 => "NO_AP_FOUND_IN_AUTHMODE_THRESHOLD",
        212 => "NO_AP_FOUND_IN_RSSI_THRESHOLD",
        _ => "UNKNOWN",
    }
}
