use std::path::Path;
use std::collections::HashMap;
use super::models::Site;

#[derive(Debug, serde::Serialize)]
pub struct SiteInfo {
    pub app_name: String,
    pub app_env: String,
    pub app_debug: bool,
    pub app_url: String,
    pub app_timezone: String,
    pub app_locale: String,
    pub framework_name: String,
    pub framework_version: String,
    pub maintenance_mode: bool,
}

impl Default for SiteInfo {
    fn default() -> Self {
        Self {
            app_name: String::new(), app_env: String::new(),
            app_debug: false, app_url: String::new(),
            app_timezone: String::new(), app_locale: String::new(),
            framework_name: String::new(), framework_version: String::new(),
            maintenance_mode: false,
        }
    }
}

pub fn read(site: &Site) -> SiteInfo {
    let p = Path::new(&site.path);
    match site.project_type.as_str() {
        "laravel"       => read_laravel(p),
        "wordpress"     => read_wordpress(p),
        "codeigniter4"  => read_codeigniter4(p),
        "codeigniter3"  => read_codeigniter3(p),
        _               => SiteInfo::default(),
    }
}

fn read_laravel(root: &Path) -> SiteInfo {
    let mut info = SiteInfo {
        framework_name: "Laravel".into(),
        ..Default::default()
    };

    // Read version from vendor/laravel/framework/src/Illuminate/Foundation/Application.php
    let version_file = root
        .join("vendor/laravel/framework/src/Illuminate/Foundation/Application.php");
    if let Ok(contents) = std::fs::read_to_string(&version_file) {
        if let Some(ver) = extract_php_const(&contents, "VERSION") {
            info.framework_version = ver;
        }
    }

    // Read .env
    let env = read_dotenv(&root.join(".env"));
    info.app_name     = env.get("APP_NAME").cloned().unwrap_or_default().trim_matches('\'').trim_matches('"').to_string();
    info.app_env      = env.get("APP_ENV").cloned().unwrap_or_default();
    info.app_debug    = env.get("APP_DEBUG").map(|v| v == "true").unwrap_or(false);
    info.app_url      = env.get("APP_URL").cloned().unwrap_or_default();
    info.app_timezone = env.get("APP_TIMEZONE").cloned().unwrap_or_default();
    info.app_locale   = env.get("APP_LOCALE").or_else(|| env.get("APP_FAKER_LOCALE")).cloned().unwrap_or_default();

    // Maintenance mode: storage/framework/down or .maintenance file
    info.maintenance_mode =
        root.join("storage/framework/down").exists() ||
        root.join(".maintenance").exists();

    info
}

fn read_wordpress(root: &Path) -> SiteInfo {
    let mut info = SiteInfo {
        framework_name: "WordPress".into(),
        ..Default::default()
    };

    let cfg = root.join("wp-config.php");
    if let Ok(contents) = std::fs::read_to_string(&cfg) {
        info.app_name = extract_php_define(&contents, "DB_NAME")
            .unwrap_or_default();
        // WP doesn't have a single APP_URL define but uses siteurl option
        info.app_url = extract_php_define(&contents, "WP_HOME")
            .or_else(|| extract_php_define(&contents, "WP_SITEURL"))
            .unwrap_or_default();
    }

    // Version from wp-includes/version.php
    let ver_file = root.join("wp-includes/version.php");
    if let Ok(contents) = std::fs::read_to_string(&ver_file) {
        info.framework_version = extract_php_var(&contents, "wp_version")
            .unwrap_or_default();
    }

    info
}

fn read_codeigniter4(root: &Path) -> SiteInfo {
    let mut info = SiteInfo {
        framework_name: "CodeIgniter".into(),
        ..Default::default()
    };

    // CI4: app/Config/App.php
    let cfg = root.join("app/Config/App.php");
    if let Ok(contents) = std::fs::read_to_string(&cfg) {
        info.app_url = extract_php_prop(&contents, "baseURL").unwrap_or_default();
        info.app_timezone = extract_php_prop(&contents, "appTimezone").unwrap_or_default();
        info.app_locale = extract_php_prop(&contents, "defaultLocale").unwrap_or_default();
    }

    // CI4 version from system/CodeIgniter.php
    let ver_file = root.join("system/CodeIgniter.php");
    if let Ok(contents) = std::fs::read_to_string(&ver_file) {
        info.framework_version = extract_php_const(&contents, "CI_VERSION")
            .unwrap_or_default();
    }

    info
}

fn read_codeigniter3(root: &Path) -> SiteInfo {
    let mut info = SiteInfo {
        framework_name: "CodeIgniter".into(),
        framework_version: "3".into(),
        ..Default::default()
    };
    // CI3: application/config/config.php
    let cfg = root.join("application/config/config.php");
    if let Ok(contents) = std::fs::read_to_string(&cfg) {
        info.app_url = extract_php_array_key(&contents, "base_url").unwrap_or_default();
        info.app_timezone = extract_php_array_key(&contents, "time_reference").unwrap_or_default();
        info.app_locale = extract_php_array_key(&contents, "language").unwrap_or_default();
    }
    // CI3 version from system/core/CodeIgniter.php
    let ver_file = root.join("system/core/CodeIgniter.php");
    if let Ok(contents) = std::fs::read_to_string(&ver_file) {
        info.framework_version = extract_php_const(&contents, "CI_VERSION").unwrap_or("3".into());
    }
    info
}

// ── Parsers ───────────────────────────────────────────────────────────────────

fn read_dotenv(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(contents) = std::fs::read_to_string(path) {
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            if let Some((k, v)) = line.split_once('=') {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                map.insert(k.trim().to_string(), v.to_string());
            }
        }
    }
    map
}

fn extract_php_define(src: &str, name: &str) -> Option<String> {
    let needle = format!("define('{}',", name);
    let needle2 = format!("define(\"{}\",", name);
    let line = src.lines().find(|l| l.contains(&needle) || l.contains(&needle2))?;
    let after = line.split(',').nth(1)?;
    Some(after.trim().trim_matches(')').trim().trim_matches('"').trim_matches('\'').to_string())
}

fn extract_php_var(src: &str, name: &str) -> Option<String> {
    let needle = format!("${} =", name);
    let line = src.lines().find(|l| l.contains(&needle))?;
    let after = line.split('=').nth(1)?;
    Some(after.trim().trim_matches(';').trim().trim_matches('"').trim_matches('\'').to_string())
}

fn extract_php_const(src: &str, name: &str) -> Option<String> {
    let needle = format!("const {} =", name);
    let line = src.lines().find(|l| l.contains(&needle))?;
    let after = line.split('=').nth(1)?;
    Some(after.trim().trim_matches(';').trim().trim_matches('"').trim_matches('\'').to_string())
}

fn extract_php_prop(src: &str, name: &str) -> Option<String> {
    let needle = format!("${} =", name);
    let line = src.lines().find(|l| l.contains(&needle))?;
    let after = line.split('=').nth(1)?;
    Some(after.trim().trim_matches(';').trim().trim_matches('"').trim_matches('\'').to_string())
}

fn extract_php_array_key(src: &str, key: &str) -> Option<String> {
    let needle = format!("['{}']", key);
    let line = src.lines().find(|l| l.contains(&needle) && l.contains('='))?;
    let after = line.split('=').nth(1)?;
    Some(after.trim().trim_matches(';').trim().trim_matches('"').trim_matches('\'').to_string())
}
