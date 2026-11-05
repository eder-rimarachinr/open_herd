use crate::infrastructure::dto::{CatalogEntry, PhpVersion};

/// The Microsoft Visual C++ runtime a PHP build links against. Windows has to
/// have it installed or `php.exe` / `php-cgi.exe` won't even start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcRuntime {
    /// VC11 builds (PHP 5.5 / 5.6): `msvcr110.dll`.
    Vc2012,
    /// VC14, VC15, VS16, VS17 builds (PHP 7.0+): `vcruntime140.dll`.
    Vc2015To2022,
}

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

impl PhpLine {
    pub fn filename(&self) -> String {
        format!("php-{}-nts-Win32-{}-x64.zip", self.version, self.build)
    }

    /// PHP before 7.2 only understands `extension=php_<name>.dll` in php.ini.
    pub fn legacy_extension_names(&self) -> bool {
        matches!(self.major, "5.6" | "7.0" | "7.1")
    }
}

const fn line(major: &'static str, version: &'static str, build: &'static str, sha256: &'static str, security_only: bool, end_of_life: bool, runtime: VcRuntime) -> PhpLine {
    PhpLine { major, version, build, sha256, security_only, end_of_life, runtime }
}

use VcRuntime::{Vc2012, Vc2015To2022};

/// Newest first. The end-of-life builds come from windows.php.net's archive;
/// their hashes were taken from the files served there (it publishes none).
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

pub fn php_line(major: &str) -> Option<&'static PhpLine> {
    PHP_LINES.iter().find(|l| l.major == major)
}

pub fn build_catalog(versions: &[PhpVersion], running: &[String]) -> Vec<CatalogEntry> {
    PHP_LINES.iter().map(|l| {
        let installed_ver = versions.iter().find(|v| v.major == l.major);
        CatalogEntry {
            major: l.major.to_string(),
            latest_patch: l.version.to_string(),
            installed_patch: installed_ver.map(|v| v.version.clone()),
            installed: installed_ver.is_some(),
            running: running.iter().any(|r| r == l.major),
            has_update: installed_ver.is_some_and(|v| v.version != l.version),
            security_only: l.security_only,
            end_of_life: l.end_of_life,
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
    fn catalog_marks_installed_running_and_updates() {
        let installed = vec![PhpVersion {
            version: "7.3.10".into(), major: "7.3".into(), binary_path: String::new(), fpm_binary: String::new(),
            fastcgi_addr: String::new(), installed: true, running: true,
        }];
        let catalog = build_catalog(&installed, &["7.3".to_string()]);
        assert_eq!(catalog.len(), PHP_LINES.len());
        let e = catalog.iter().find(|e| e.major == "7.3").unwrap();
        assert!(e.installed && e.running && e.has_update && e.end_of_life);
        assert_eq!(e.installed_patch.as_deref(), Some("7.3.10"));
        assert!(!catalog.iter().find(|e| e.major == "8.5").unwrap().installed);
    }
}
