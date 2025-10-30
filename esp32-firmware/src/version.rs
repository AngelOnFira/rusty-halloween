use anyhow::{Context, Result};
use serde::Deserialize;

/// Current firmware version - automatically pulled from Cargo.toml
/// Update the version in Cargo.toml [package] section when releasing
// pub const FIRMWARE_VERSION: &str = env!("CARGO_PKG_VERSION");

// Uncomment to override for OTA testing (will detect any GitHub release as "newer"):
pub const FIRMWARE_VERSION: &str = "0.0.1";

/// Build timestamp - automatically set at compile time
pub const BUILD_TIMESTAMP: &str = env!("BUILD_TIMESTAMP");

/// GitHub repository information
pub const GITHUB_REPO_OWNER: &str = "AngelOnFira";
pub const GITHUB_REPO_NAME: &str = "rusty-halloween";

/// Show server URL for firmware distribution
pub const SHOW_SERVER_URL: &str = "https://rusty-halloween-show-server.rustwood.org";

/// Semantic version structure for comparison
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Parse a semantic version string (e.g., "esp32-v0.1.1")
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim_start_matches("esp32-v");
        let parts: Vec<&str> = s.split('.').collect();

        if parts.len() != 3 {
            anyhow::bail!("Invalid version format: {}", s);
        }

        Ok(Version {
            major: parts[0].parse().context("Invalid major version")?,
            minor: parts[1].parse().context("Invalid minor version")?,
            patch: parts[2].parse().context("Invalid patch version")?,
        })
    }

    /// Get current firmware version
    pub fn current() -> Result<Self> {
        Self::parse(FIRMWARE_VERSION)
    }

    /// Compare if this version is newer than another
    pub fn is_newer_than(&self, other: &Version) -> bool {
        self > other
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Firmware release metadata from show server
#[derive(Debug, Deserialize)]
pub struct GitHubRelease {
    pub version: String,  // Changed from tag_name to match server API
    pub name: String,
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub download_url: String,  // Changed from browser_download_url - simplified URL from server
    pub size: u64,
}

impl GitHubRelease {
    /// Get the firmware asset from this release
    pub fn get_firmware_asset(&self) -> Option<&GitHubAsset> {
        self.assets
            .iter()
            .find(|asset| asset.name.ends_with(".bin"))
    }

    /// Parse the version from the version field
    pub fn version(&self) -> Result<Version> {
        Version::parse(&self.version)
    }
}

/// Check GitHub for the latest release
pub fn check_github_for_updates() -> Result<Option<GitHubRelease>> {
    info!("version: Checking GitHub for firmware updates...");

    // Note: This requires HTTP client implementation
    // For now, this is a placeholder that will be implemented with the OTA HTTP client

    warn!("version: GitHub update checking not yet implemented - requires HTTP client");
    Ok(None)
}

/// Check if an update is available
pub fn is_update_available(latest_version: &Version) -> Result<bool> {
    let current = Version::current()?;
    info!(
        "Version check: Current={}, Latest={}",
        current, latest_version
    );
    Ok(latest_version.is_newer_than(&current))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        let v1 = Version::parse("0.1.0").unwrap();
        assert_eq!(v1.major, 0);
        assert_eq!(v1.minor, 1);
        assert_eq!(v1.patch, 0);

        let v2 = Version::parse("v1.2.3").unwrap();
        assert_eq!(v2.major, 1);
        assert_eq!(v2.minor, 2);
        assert_eq!(v2.patch, 3);
    }

    #[test]
    fn test_version_comparison() {
        let v1 = Version::parse("0.1.0").unwrap();
        let v2 = Version::parse("0.2.0").unwrap();
        let v3 = Version::parse("1.0.0").unwrap();

        assert!(v2.is_newer_than(&v1));
        assert!(v3.is_newer_than(&v2));
        assert!(!v1.is_newer_than(&v2));
    }
}
