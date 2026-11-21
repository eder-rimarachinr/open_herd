//! Composer: the release value object, the rule that picks which release fits
//! a PHP version, and the PHP extensions Composer needs.

use super::errors::DomainError;

/// Extensions Composer needs to download packages: TLS, multibyte strings,
/// HTTP (curl is optional for Composer but makes it much faster) and zip
/// archives (without it, every package is cloned with git).
pub const REQUIRED_EXTENSIONS: [&str; 4] = ["openssl", "mbstring", "curl", "zip"];

/// A Composer release from getcomposer.org's `versions` list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerRelease {
    /// `2.11.2`. Validated (digits and dots) so it can be joined into URLs.
    pub version: String,
    /// Oldest PHP it runs on, as `PHP_VERSION_ID` (`70205` = 7.2.5).
    pub min_php: u32,
    /// Long-term-support line (2.2.x, the last one for PHP < 7.2.5).
    pub lts: bool,
}

impl ComposerRelease {
    pub fn new(version: &str, min_php: u32, lts: bool) -> Result<Self, DomainError> {
        if !is_release_version(version) {
            return Err(DomainError::InvalidComposerVersion(version.to_string()));
        }
        Ok(Self { version: version.to_string(), min_php, lts })
    }
}

/// `2.11.2`-style: 2 to 4 numeric parts, nothing else.
fn is_release_version(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    (2..=4).contains(&parts.len())
        && parts.iter().all(|p| !p.is_empty() && p.len() <= 6 && p.bytes().all(|b| b.is_ascii_digit()))
}

/// `PHP_VERSION_ID` of a PHP version string: `8.2.18` → 80218, `7.2` → 70200.
/// `None` if it does not start with `major.minor`.
pub fn php_version_id(version: &str) -> Option<u32> {
    let mut nums = version.split('.').map(|p| {
        let digits: String = p.chars().take_while(char::is_ascii_digit).collect();
        digits.parse::<u32>().ok()
    });
    let major = nums.next()??;
    let minor = nums.next()??;
    let patch = nums.next().flatten().unwrap_or(0);
    Some(major * 10_000 + minor.min(99) * 100 + patch.min(99))
}

/// Human form of a `PHP_VERSION_ID`: 70205 → `7.2.5`.
pub fn php_version_label(id: u32) -> String {
    format!("{}.{}.{}", id / 10_000, (id / 100) % 100, id % 100)
}

/// The newest release that runs on `php_id`. getcomposer.org lists the
/// newest first and the 2.2 LTS (PHP 5.3+) after it.
pub fn select_release(releases: &[ComposerRelease], php_id: u32) -> Option<&ComposerRelease> {
    releases.iter().filter(|r| r.min_php <= php_id).max_by(|a, b| compare_versions(&a.version, &b.version))
}

/// Numeric comparison of dotted versions (`2.11.2` > `2.2.32`).
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let nums = |v: &str| v.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    nums(a).cmp(&nums(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(v: &str, min: u32) -> ComposerRelease { ComposerRelease::new(v, min, false).unwrap() }

    #[test]
    fn version_ids() {
        assert_eq!(php_version_id("8.2.18"), Some(80218));
        assert_eq!(php_version_id("7.2.5"), Some(70205));
        assert_eq!(php_version_id("8.5.0RC1"), Some(80500));
        assert_eq!(php_version_id("5.6"), Some(50600));
        assert_eq!(php_version_id("x"), None);
        assert_eq!(php_version_label(70205), "7.2.5");
    }

    #[test]
    fn picks_newest_release_the_php_can_run() {
        let list = [rel("2.11.2", 70205), rel("2.2.32", 50300)];
        assert_eq!(select_release(&list, 80400).map(|r| r.version.as_str()), Some("2.11.2"));
        assert_eq!(select_release(&list, 70204).map(|r| r.version.as_str()), Some("2.2.32"), "PHP 7.2.4 gets the LTS");
        assert_eq!(select_release(&list, 50600).map(|r| r.version.as_str()), Some("2.2.32"));
        assert_eq!(select_release(&list, 50200), None);
    }

    #[test]
    fn rejects_versions_unsafe_for_urls() {
        assert!(ComposerRelease::new("2.11.2", 0, false).is_ok());
        for bad in ["", "2", "../2.1", "2.1/x", "8a53515e753d9f60f1ed338cd40e0fae9464af52", "2.1.0-RC1"] {
            assert!(ComposerRelease::new(bad, 0, false).is_err(), "{bad}");
        }
    }

    #[test]
    fn compares_numerically() {
        assert_eq!(compare_versions("2.11.2", "2.2.32"), std::cmp::Ordering::Greater);
        assert_eq!(compare_versions("2.2.32", "2.2.32"), std::cmp::Ordering::Equal);
    }
}
