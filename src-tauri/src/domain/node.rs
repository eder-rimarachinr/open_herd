//! Node.js versions: the version value object and the rule that picks which
//! releases from nodejs.org's `index.json` are worth offering.

use chrono::NaiveDate;
use std::cmp::Ordering;

use super::errors::DomainError;

/// An exact Node.js release (`22.19.0`). Validated so it can be safely joined
/// into paths and URLs: digits and two dots only, no prerelease suffix.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeVersion {
    text: String,
    parts: (u32, u32, u32),
}

impl NodeVersion {
    /// Accepts `22.19.0` or `v22.19.0`.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let invalid = || DomainError::InvalidNodeVersion(raw.to_string());
        let s = raw.strip_prefix('v').unwrap_or(raw);
        let mut nums = [0u32; 3];
        let mut pieces = s.split('.');
        for slot in &mut nums {
            let piece = pieces.next().ok_or_else(invalid)?;
            if piece.is_empty() || piece.len() > 6 || !piece.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid());
            }
            *slot = piece.parse().map_err(|_| invalid())?;
        }
        if pieces.next().is_some() { return Err(invalid()); }
        Ok(Self { text: format!("{}.{}.{}", nums[0], nums[1], nums[2]), parts: (nums[0], nums[1], nums[2]) })
    }

    pub fn as_str(&self) -> &str { &self.text }
    /// Folder name used by nvm-windows and by Open Herd: `v22.19.0`.
    pub fn dir_name(&self) -> String { format!("v{}", self.text) }
    pub fn major(&self) -> u32 { self.parts.0 }
}

impl std::fmt::Display for NodeVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.text) }
}

impl Ord for NodeVersion {
    fn cmp(&self, other: &Self) -> Ordering { self.parts.cmp(&other.parts) }
}
impl PartialOrd for NodeVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

/// One release from `index.json`, reduced to what Open Herd uses.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeReleaseInfo {
    pub version: NodeVersion,
    pub date: NaiveDate,
    /// LTS codename (`"Jod"`), `None` for non-LTS releases.
    pub lts: Option<String>,
    pub security: bool,
    pub has_win_x64_zip: bool,
}

/// A line still gets releases every few weeks; one silent for this long is
/// treated as no longer maintained (`index.json` has no end-of-life dates).
const MAINTAINED_WITHIN_DAYS: i64 = 183;

/// Newest release of each LTS line that is still maintained, plus the newest
/// non-LTS ("Current") line when it is newer than every LTS line. Newest first.
pub fn select_stable_releases(all: &[NodeReleaseInfo], today: NaiveDate) -> Vec<NodeReleaseInfo> {
    let mut newest_per_major: Vec<&NodeReleaseInfo> = Vec::new();
    for r in all.iter().filter(|r| r.has_win_x64_zip) {
        match newest_per_major.iter_mut().find(|n| n.version.major() == r.version.major()) {
            Some(slot) => if r.version > slot.version { *slot = r; },
            None => newest_per_major.push(r),
        }
    }
    let maintained = |r: &NodeReleaseInfo| (today - r.date).num_days() < MAINTAINED_WITHIN_DAYS;
    let mut out: Vec<NodeReleaseInfo> = newest_per_major.iter()
        .filter(|r| r.lts.is_some() && maintained(r))
        .map(|r| (*r).clone())
        .collect();
    let newest_lts_major = out.iter().map(|r| r.version.major()).max().unwrap_or(0);
    if let Some(current) = newest_per_major.iter()
        .filter(|r| r.lts.is_none() && r.version.major() > newest_lts_major && maintained(r))
        .max_by(|a, b| a.version.cmp(&b.version))
    {
        out.push((*current).clone());
    }
    out.sort_by(|a, b| b.version.cmp(&a.version));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> NodeVersion { NodeVersion::parse(s).unwrap() }
    fn d(s: &str) -> NaiveDate { NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap() }
    fn rel(ver: &str, date: &str, lts: Option<&str>, zip: bool) -> NodeReleaseInfo {
        NodeReleaseInfo { version: v(ver), date: d(date), lts: lts.map(String::from), security: false, has_win_x64_zip: zip }
    }

    #[test]
    fn parse_accepts_with_and_without_v() {
        let a = v("v22.19.0");
        assert_eq!(a.as_str(), "22.19.0");
        assert_eq!(a.dir_name(), "v22.19.0");
        assert_eq!(a.major(), 22);
        assert_eq!(v("22.19.0"), a);
    }

    #[test]
    fn parse_rejects_traversal_and_partial() {
        for bad in ["..\\x", "1.2", "22.19.0-rc.1", "", "22.19.0/..", "v", "1.2.3.4", "a.b.c", "1..2"] {
            assert_eq!(NodeVersion::parse(bad), Err(DomainError::InvalidNodeVersion(bad.into())), "{bad}");
        }
    }

    #[test]
    fn ordering_is_numeric() {
        assert!(v("10.0.0") > v("9.11.2"));
        assert!(v("22.10.0") > v("22.9.9"));
    }

    fn fixtures() -> Vec<NodeReleaseInfo> {
        vec![
            rel("26.13.0", "2026-10-28", Some("Lithium"), true),
            rel("26.12.1", "2026-10-21", None, true),
            rel("24.21.0", "2026-10-01", Some("Krypton"), true),
            rel("24.22.0", "2026-10-20", Some("Krypton"), false), // no Windows zip: ignored
            rel("22.21.0", "2026-08-01", Some("Jod"), true),
            rel("20.19.5", "2026-03-01", Some("Iron"), true),     // > 183 days old
            rel("25.9.0",  "2026-04-10", None, true),             // Current older than LTS 26
        ]
    }

    fn versions(rs: &[NodeReleaseInfo]) -> Vec<&str> { rs.iter().map(|r| r.version.as_str()).collect() }

    #[test]
    fn filter_keeps_maintained_lts_lines_newest_first() {
        let picked = select_stable_releases(&fixtures(), d("2026-11-05"));
        assert_eq!(versions(&picked), ["26.13.0", "24.21.0", "22.21.0"]);
    }

    #[test]
    fn filter_includes_current_when_newer_than_lts() {
        let mut all = fixtures();
        all.push(rel("27.1.0", "2026-11-01", None, true));
        all.push(rel("27.0.0", "2026-10-30", None, true));
        let picked = select_stable_releases(&all, d("2026-11-05"));
        assert_eq!(versions(&picked), ["27.1.0", "26.13.0", "24.21.0", "22.21.0"]);
    }
}
