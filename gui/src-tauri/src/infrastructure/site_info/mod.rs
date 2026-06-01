use std::path::Path;
use std::collections::HashMap;
use crate::infrastructure::dto::Site;

#[derive(Debug, serde::Serialize)]
pub struct SiteInfo {
    pub app_name: String, pub app_env: String, pub app_debug: bool,
    pub app_url: String, pub app_timezone: String, pub app_locale: String,
    pub framework_name: String, pub framework_version: String, pub maintenance_mode: bool,
}

impl Default for SiteInfo {
    fn default() -> Self {
        Self { app_name: String::new(), app_env: String::new(), app_debug: false,
               app_url: String::new(), app_timezone: String::new(), app_locale: String::new(),
               framework_name: String::new(), framework_version: String::new(), maintenance_mode: false }
    }
}

pub fn read(site: &Site) -> SiteInfo {
    let p = Path::new(&site.path);
    match site.project_type.as_str() {
        "laravel"      => read_laravel(p),
        "wordpress"    => read_wordpress(p),
        "codeigniter4" => read_codeigniter4(p),
        "codeigniter3" => read_codeigniter3(p),
        _              => SiteInfo::default(),
    }
}

fn read_laravel(root: &Path) -> SiteInfo {
    let mut info = SiteInfo { framework_name: "Laravel".into(), ..Default::default() };
    let ver_file = root.join("vendor/laravel/framework/src/Illuminate/Foundation/Application.php");
    if let Ok(c) = std::fs::read_to_string(&ver_file) { if let Some(v) = extract_php_const(&c, "VERSION") { info.framework_version = v; } }
    let env = read_dotenv(&root.join(".env"));
    info.app_name  = env.get("APP_NAME").cloned().unwrap_or_default().trim_matches('\'').trim_matches('"').to_string();
    info.app_env   = env.get("APP_ENV").cloned().unwrap_or_default();
    info.app_debug = env.get("APP_DEBUG").map(|v| v == "true").unwrap_or(false);
    info.app_url   = env.get("APP_URL").cloned().unwrap_or_default();
    info.app_timezone = env.get("APP_TIMEZONE").cloned().unwrap_or_default();
    info.app_locale   = env.get("APP_LOCALE").or_else(|| env.get("APP_FAKER_LOCALE")).cloned().unwrap_or_default();
    info.maintenance_mode = root.join("storage/framework/down").exists() || root.join(".maintenance").exists();
    info
}

fn read_wordpress(root: &Path) -> SiteInfo {
    let mut info = SiteInfo { framework_name: "WordPress".into(), ..Default::default() };
    if let Ok(c) = std::fs::read_to_string(root.join("wp-config.php")) {
        info.app_name = extract_php_define(&c, "DB_NAME").unwrap_or_default();
        info.app_url  = extract_php_define(&c, "WP_HOME").or_else(|| extract_php_define(&c, "WP_SITEURL")).unwrap_or_default();
    }
    if let Ok(c) = std::fs::read_to_string(root.join("wp-includes/version.php")) { info.framework_version = extract_php_var(&c, "wp_version").unwrap_or_default(); }
    info
}

fn read_codeigniter4(root: &Path) -> SiteInfo {
    let mut info = SiteInfo { framework_name: "CodeIgniter".into(), ..Default::default() };
    if let Ok(c) = std::fs::read_to_string(root.join("app/Config/App.php")) {
        info.app_url      = extract_php_prop(&c, "baseURL").unwrap_or_default();
        info.app_timezone = extract_php_prop(&c, "appTimezone").unwrap_or_default();
        info.app_locale   = extract_php_prop(&c, "defaultLocale").unwrap_or_default();
    }
    if let Ok(c) = std::fs::read_to_string(root.join("system/CodeIgniter.php")) { info.framework_version = extract_php_const(&c, "CI_VERSION").unwrap_or_default(); }
    info
}

fn read_codeigniter3(root: &Path) -> SiteInfo {
    let mut info = SiteInfo { framework_name: "CodeIgniter".into(), framework_version: "3".into(), ..Default::default() };
    if let Ok(c) = std::fs::read_to_string(root.join("application/config/config.php")) {
        info.app_url      = extract_php_array_key(&c, "base_url").unwrap_or_default();
        info.app_timezone = extract_php_array_key(&c, "time_reference").unwrap_or_default();
        info.app_locale   = extract_php_array_key(&c, "language").unwrap_or_default();
    }
    if let Ok(c) = std::fs::read_to_string(root.join("system/core/CodeIgniter.php")) { info.framework_version = extract_php_const(&c, "CI_VERSION").unwrap_or("3".into()); }
    info
}

fn read_dotenv(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(contents) = std::fs::read_to_string(path) {
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            if let Some((k, v)) = line.split_once('=') { map.insert(k.trim().to_string(), v.trim().trim_matches('"').trim_matches('\'').to_string()); }
        }
    }
    map
}

fn extract_php_define(src: &str, name: &str) -> Option<String> {
    let line = src.lines().find(|l| l.contains(&format!("define('{}',", name)) || l.contains(&format!("define(\"{}\",", name)))?;
    Some(line.split(',').nth(1)?.trim().trim_matches(')').trim().trim_matches('"').trim_matches('\'').to_string())
}
fn extract_php_var(src: &str, name: &str) -> Option<String> {
    let line = src.lines().find(|l| l.contains(&format!("${} =", name)))?;
    Some(line.split('=').nth(1)?.trim().trim_matches(';').trim().trim_matches('"').trim_matches('\'').to_string())
}
fn extract_php_const(src: &str, name: &str) -> Option<String> {
    let line = src.lines().find(|l| l.contains(&format!("const {} =", name)))?;
    Some(line.split('=').nth(1)?.trim().trim_matches(';').trim().trim_matches('"').trim_matches('\'').to_string())
}
fn extract_php_prop(src: &str, name: &str) -> Option<String> {
    let line = src.lines().find(|l| l.contains(&format!("${} =", name)))?;
    Some(line.split('=').nth(1)?.trim().trim_matches(';').trim().trim_matches('"').trim_matches('\'').to_string())
}
fn extract_php_array_key(src: &str, key: &str) -> Option<String> {
    let line = src.lines().find(|l| l.contains(&format!("['{}']", key)) && l.contains('='))?;
    Some(line.split('=').nth(1)?.trim().trim_matches(';').trim().trim_matches('"').trim_matches('\'').to_string())
}
