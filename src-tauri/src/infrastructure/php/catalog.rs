use crate::infrastructure::dto::{CatalogEntry, PhpVersion};

// ── Windows: NTS x64 zips from windows.php.net ─────────────────────────────────

/// The Microsoft Visual C++ runtime a PHP build links against. Windows has to
/// have it installed or `php.exe` / `php-cgi.exe` won't even start.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcRuntime {
    /// VC11 builds (PHP 5.5 / 5.6): `msvcr110.dll`.
    Vc2012,
    /// VC14, VC15, VS16, VS17 builds (PHP 7.0+): `vcruntime140.dll`.
    Vc2015To2022,
}

#[cfg(any(windows, test))]
impl VcRuntime {
    pub fn label(self) -> &'static str {
        match self {
            Self::Vc2012 => "Microsoft Visual C++ 2012 Redistributable (x64)",
            Self::Vc2015To2022 => "Microsoft Visual C++ 2015-2022 Redistributable (x64)",
        }
    }

    pub fn download_url(self) -> &'static str {
        match self {
            Self::Vc2012 => "https://www.microsoft.com/en-us/download/details.aspx?id=30679",
            Self::Vc2015To2022 => "https://aka.ms/vs/17/release/vc_redist.x64.exe",
        }
    }
}

/// One PHP release line Open Herd can install: the newest patch, the exact NTS
/// x64 build on windows.php.net and its pinned SHA-256 (nothing unpinned is
/// ever installed).
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy)]
pub struct PhpLine {
    pub major: &'static str,
    pub version: &'static str,
    /// Compiler tag exactly as spelled in the file name (`VC15` up to 7.3, `vc15` for 7.4).
    pub build: &'static str,
    pub sha256: &'static str,
    pub security_only: bool,
    pub end_of_life: bool,
    pub runtime: VcRuntime,
}

#[cfg(any(windows, test))]
impl PhpLine {
    pub fn filename(&self) -> String {
        format!("php-{}-nts-Win32-{}-x64.zip", self.version, self.build)
    }

    /// PHP before 7.2 only understands `extension=php_<name>.dll` in php.ini.
    pub fn legacy_extension_names(&self) -> bool {
        matches!(self.major, "5.6" | "7.0" | "7.1")
    }
}

#[cfg(any(windows, test))]
const fn line(major: &'static str, version: &'static str, build: &'static str, sha256: &'static str, security_only: bool, end_of_life: bool, runtime: VcRuntime) -> PhpLine {
    PhpLine { major, version, build, sha256, security_only, end_of_life, runtime }
}

#[cfg(any(windows, test))]
use VcRuntime::{Vc2012, Vc2015To2022};

/// Newest first. The end-of-life builds come from windows.php.net's archive;
/// their hashes were taken from the files served there (it publishes none).
#[cfg(any(windows, test))]
pub const PHP_LINES: &[PhpLine] = &[
    line("8.5", "8.5.12", "vs17", "2ab242af38d76bdf417cde5ec048b1bcd76a5a40f39a3d15d5f046f38571c477", false, false, Vc2015To2022),
    line("8.4", "8.4.27", "vs17", "93bd8a815a52156606fb906d0573437908c61f96e2cc52c03a51e36a41bf5236", false, false, Vc2015To2022),
    line("8.3", "8.3.36", "vs16", "3c9242622c7402b47c38d2ec2ed779cef3a290fd6f2bde6c35f9c482699e7900", false, false, Vc2015To2022),
    line("8.2", "8.2.35", "vs16", "798ffa4b9ce22a820d57048e9695df7245bf917ed067917f256942a10918a8cc", false, false, Vc2015To2022),
    line("8.1", "8.1.34", "vs16", "9cfe246cb144076c16f5913a3ef88a474c3dd7e60f0f0c8bb95faf68674016cc", true,  false, Vc2015To2022),
    line("8.0", "8.0.30", "vs16", "dfb70498ffa2c617f2f655a155564697e3c9cca41709938fd1a5997d1d5b0785", false, true,  Vc2015To2022),
    line("7.4", "7.4.33", "vc15", "14ae3250d4447c8ccfc4c45a70d90adfbcd61e728d85f0be56a7ddf8f9c8aace", false, true,  Vc2015To2022),
    line("7.3", "7.3.33", "VC15", "5eaf3cad80e678623f222a42c99bcefcc60eea359d407fb51e805afdb3b13e5e", false, true,  Vc2015To2022),
    line("7.2", "7.2.34", "VC15", "3c673eab656e26fd6bc3ad27fe71169ad888b04e21d63d3c6b3151d5ed216563", false, true,  Vc2015To2022),
    line("7.1", "7.1.33", "VC14", "071438be5bbaee8b34894ddba7852b6040991a3c853ab8141efcb1b6655bbbef", false, true,  Vc2015To2022),
    line("7.0", "7.0.33", "VC14", "bba39bde5b0bd50efadc11e2716c7528669945f9d1397d707486e401e67e89fb", false, true,  Vc2015To2022),
    line("5.6", "5.6.40", "VC11", "3d7668280fa4b16f70705539ba1e4ea17eef344c81e82881cbeca26fb7f181f1", false, true,  Vc2012),
];

#[cfg(any(windows, test))]
pub fn php_line(major: &str) -> Option<&'static PhpLine> {
    PHP_LINES.iter().find(|l| l.major == major)
}

// ── Linux: static builds from static-php-cli ───────────────────────────────────

/// Where static-php-cli publishes its prebuilt binaries. Its "common" set has
/// the usual Laravel / WordPress extensions (curl, gd, mbstring, mysqlnd,
/// pdo_mysql, pgsql, redis, sqlite, zip, …) compiled in: there are no
/// `extension=` lines to toggle.
#[cfg(any(not(windows), test))]
pub const STATIC_PHP_BASE: &str = "https://dl.static-php.dev/static-php-cli/common";

/// One PHP line on Linux: a `php-fpm` tarball and a `php` CLI tarball for
/// x86_64, both pinned (static-php-cli publishes no checksums; the hashes were
/// taken from the files it serves). php.net has no Linux binaries of its own.
#[cfg(any(not(windows), test))]
#[derive(Debug, Clone, Copy)]
pub struct StaticPhpLine {
    pub major: &'static str,
    pub version: &'static str,
    pub fpm_sha256: &'static str,
    pub cli_sha256: &'static str,
    pub security_only: bool,
    pub end_of_life: bool,
}

#[cfg(any(not(windows), test))]
impl StaticPhpLine {
    /// `sapi` is `fpm` or `cli`.
    pub fn filename(&self, sapi: &str) -> String {
        format!("php-{}-{sapi}-linux-x86_64.tar.gz", self.version)
    }
}

#[cfg(any(not(windows), test))]
const fn static_line(major: &'static str, version: &'static str, fpm_sha256: &'static str, cli_sha256: &'static str, security_only: bool, end_of_life: bool) -> StaticPhpLine {
    StaticPhpLine { major, version, fpm_sha256, cli_sha256, security_only, end_of_life }
}

/// Newest first. static-php-cli builds 8.0+ only; older lines stay Windows-only.
#[cfg(any(not(windows), test))]
pub const STATIC_PHP_LINES: &[StaticPhpLine] = &[
    static_line("8.5", "8.5.8",  "836412ac76432e9f8e49a88c440c381acb2d262cc6dab4ba0a80468ab568a239", "517a18e0f0874de35669fe134eb3c52508e128a6d1da5bf890760cb800ada410", false, false),
    static_line("8.4", "8.4.23", "baa5ddcf2f9aabff78848cb6851bb104a4c0873347b8ee65eb9bcf4760a4d31a", "1aeed5bc7967977ca5b1da7163acd91bf9ba3ac56037045d4e91ee2ff2712bb7", false, false),
    static_line("8.3", "8.3.32", "78d9ac9d52fedacc33d428196d0e5e9ace8ac010091c334daae85a669370f861", "d7ff2bc40846f4efed2757e5929f7469c96f672fbf33eb4bb80cd695534750fe", false, false),
    static_line("8.2", "8.2.32", "6e14e116836f9eab42451619fb4d68117ecea4bc4b1efd7011aaee914033f800", "2e609bb88bae88661dbd447e409531f7c36931adff480d8a0a74e812b889d1be", false, false),
    static_line("8.1", "8.1.34", "7995c72719d6574c50a727a30c51d6e63adbd4f694150d9562489efd093f4210", "4caa8b82b574212b2f7af83cc800b23e0529b3dec85b8b2887547f909e717143", true,  false),
    static_line("8.0", "8.0.30", "2da44febe5afdf9013fc88948f6004685f8f14e2d6bd7a8417f45cf335ee501b", "1a5b1d5e60bca9e0c06557640f878e88bd52ac4eb28e8a118103867b084e8ff8", false, true),
];

#[cfg(any(not(windows), test))]
pub fn static_php_line(major: &str) -> Option<&'static StaticPhpLine> {
    STATIC_PHP_LINES.iter().find(|l| l.major == major)
}

// ── Catalog ────────────────────────────────────────────────────────────────────

/// (major, newest patch, security-only, end of life) of every line this
/// platform can install, newest first.
fn catalog_lines() -> Vec<(&'static str, &'static str, bool, bool)> {
    #[cfg(windows)]
    let lines = PHP_LINES.iter().map(|l| (l.major, l.version, l.security_only, l.end_of_life)).collect();
    #[cfg(not(windows))]
    let lines = STATIC_PHP_LINES.iter().map(|l| (l.major, l.version, l.security_only, l.end_of_life)).collect();
    lines
}

pub fn build_catalog(versions: &[PhpVersion], running: &[String]) -> Vec<CatalogEntry> {
    catalog_lines().into_iter().map(|(major, version, security_only, end_of_life)| {
        let installed_ver = versions.iter().find(|v| v.major == major);
        CatalogEntry {
            major: major.to_string(),
            latest_patch: version.to_string(),
            installed_patch: installed_ver.map(|v| v.version.clone()),
            installed: installed_ver.is_some(),
            running: running.iter().any(|r| r == major),
            has_update: installed_ver.is_some_and(|v| v.version != version),
            security_only,
            end_of_life,
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_newest_first_and_unique() {
        let majors: Vec<f32> = PHP_LINES.iter().map(|l| l.major.parse().unwrap()).collect();
        assert!(majors.windows(2).all(|w| w[0] > w[1]), "{majors:?}");
    }

    #[test]
    fn every_line_pins_a_matching_build() {
        for l in PHP_LINES {
            assert!(l.version.starts_with(&format!("{}.", l.major)), "{}", l.major);
            assert_eq!(l.sha256.len(), 64, "{}", l.major);
            assert!(l.sha256.bytes().all(|b| b.is_ascii_hexdigit()), "{}", l.major);
        }
    }

    #[test]
    fn archived_lines_reach_back_to_5_6() {
        assert_eq!(php_line("5.6").unwrap().filename(), "php-5.6.40-nts-Win32-VC11-x64.zip");
        assert_eq!(php_line("7.3").unwrap().filename(), "php-7.3.33-nts-Win32-VC15-x64.zip");
        assert_eq!(php_line("7.4").unwrap().filename(), "php-7.4.33-nts-Win32-vc15-x64.zip");
        assert_eq!(php_line("5.6").unwrap().runtime, VcRuntime::Vc2012);
        assert!(php_line("7.1").unwrap().legacy_extension_names());
        assert!(!php_line("7.2").unwrap().legacy_extension_names());
        assert!(PHP_LINES.iter().filter(|l| l.major.starts_with('5') || l.major.starts_with('7')).all(|l| l.end_of_life));
    }

    #[test]
    fn static_lines_pin_both_tarballs() {
        let majors: Vec<f32> = STATIC_PHP_LINES.iter().map(|l| l.major.parse().unwrap()).collect();
        assert!(majors.windows(2).all(|w| w[0] > w[1]), "{majors:?}");
        for l in STATIC_PHP_LINES {
            assert!(l.version.starts_with(&format!("{}.", l.major)), "{}", l.major);
            for h in [l.fpm_sha256, l.cli_sha256] {
                assert!(h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()), "{}", l.major);
            }
            assert_ne!(l.fpm_sha256, l.cli_sha256, "{}", l.major);
            // Same support status as the Windows build of that line.
            let win = php_line(l.major).unwrap();
            assert_eq!((l.security_only, l.end_of_life), (win.security_only, win.end_of_life), "{}", l.major);
        }
        assert_eq!(static_php_line("8.4").unwrap().filename("fpm"), "php-8.4.23-fpm-linux-x86_64.tar.gz");
        assert!(static_php_line("7.4").is_none());
    }

    #[test]
    fn catalog_marks_installed_running_and_updates() {
        let installed = vec![PhpVersion {
            version: "8.0.10".into(), major: "8.0".into(), binary_path: String::new(), fpm_binary: String::new(),
            fastcgi_addr: String::new(), installed: true, running: true,
        }];
        let catalog = build_catalog(&installed, &["8.0".to_string()]);
        assert_eq!(catalog.len(), catalog_lines().len());
        let e = catalog.iter().find(|e| e.major == "8.0").unwrap();
        assert!(e.installed && e.running && e.has_update && e.end_of_life);
        assert_eq!(e.installed_patch.as_deref(), Some("8.0.10"));
        assert!(!catalog.iter().find(|e| e.major == "8.5").unwrap().installed);
    }
}
