/// Validate a domain before writing it into /etc/hosts or an nginx config.
/// Only ASCII alphanumeric + hyphens + dots, must end with .test.
pub fn domain(d: &str) -> Result<(), String> {
    if d.is_empty() {
        return Err("domain cannot be empty".into());
    }
    if !d.ends_with(".test") {
        return Err("domain must end with .test".into());
    }
    if !d.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
        return Err("domain must contain only ASCII letters, digits, hyphens, and dots".into());
    }
    // No empty labels (e.g. "foo..test")
    if d.split('.').any(|label| label.is_empty()) {
        return Err("domain has empty label".into());
    }
    Ok(())
}

/// Validate a filesystem path before using it as a site root.
/// Must be absolute and contain no control characters or null bytes.
pub fn site_path(p: &str) -> Result<(), String> {
    if p.is_empty() {
        return Err("path cannot be empty".into());
    }
    if p.chars().any(|c| c.is_control()) {
        return Err("path contains control characters".into());
    }
    if !std::path::Path::new(p).is_absolute() {
        return Err("path must be absolute".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_valid() {
        assert!(domain("myapp.test").is_ok());
        assert!(domain("my-app.test").is_ok());
        assert!(domain("sub.myapp.test").is_ok());
    }

    #[test]
    fn test_domain_injection() {
        // Newline injection
        assert!(domain("evil.test\nreturn 200;").is_err());
        // Semicolon
        assert!(domain("evil.test; return 200").is_err());
        // Non .test TLD
        assert!(domain("evil.com").is_err());
        // Empty label
        assert!(domain("evil..test").is_err());
    }

    #[test]
    fn test_path_valid() {
        #[cfg(target_os = "windows")]
        assert!(site_path(r"C:\Users\alice\projects\myapp").is_ok());
        #[cfg(not(target_os = "windows"))]
        assert!(site_path("/home/alice/projects/myapp").is_ok());
    }

    #[test]
    fn test_path_relative() {
        assert!(site_path("relative/path").is_err());
    }

    #[test]
    fn test_path_control_chars() {
        assert!(site_path("/home/alice/proj\nect").is_err());
    }
}
