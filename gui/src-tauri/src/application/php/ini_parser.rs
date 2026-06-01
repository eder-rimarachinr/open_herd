/// Parsing y edición de php.ini — extraído de `daemon/routes.rs`.
/// Funciones puras: sin I/O, sin dependencias externas.
use crate::infrastructure::dto::{PhpExtension, PhpSetting};

// ── Extensiones ───────────────────────────────────────────────────────────────

const KNOWN_EXTENSIONS: &[(&str, &str)] = &[
    ("mysqli",     "database"), ("pdo_mysql",  "database"), ("pdo_pgsql",  "database"),
    ("pdo_sqlite", "database"), ("pdo_oci",    "database"), ("oci8_12c",   "database"),
    ("sqlite3",    "database"), ("mbstring",   "string"),   ("iconv",      "string"),
    ("intl",       "string"),   ("gettext",    "string"),   ("ctype",      "string"),
    ("pspell",     "string"),   ("enchant",    "string"),   ("gd",         "image"),
    ("exif",       "image"),    ("imagick",    "image"),    ("curl",       "network"),
    ("soap",       "network"),  ("ldap",       "network"),  ("sockets",    "network"),
    ("ftp",        "network"),  ("zip",        "files"),    ("zlib",       "files"),
    ("bz2",        "files"),    ("fileinfo",   "files"),    ("openssl",    "security"),
    ("sodium",     "security"), ("hash",       "security"), ("bcmath",     "math"),
    ("gmp",        "math"),     ("calendar",   "misc"),     ("pcntl",      "misc"),
    ("shmop",      "misc"),     ("sysvmsg",    "misc"),     ("sysvsem",    "misc"),
    ("sysvshm",    "misc"),     ("xml",        "misc"),     ("xmlrpc",     "misc"),
    ("xsl",        "misc"),     ("dom",        "misc"),     ("simplexml",  "misc"),
    ("tokenizer",  "misc"),
];

pub fn parse_extensions(content: &str) -> Vec<PhpExtension> {
    KNOWN_EXTENSIONS.iter().map(|(name, category)| {
        let enabled = content.lines().any(|line| {
            let t = line.trim();
            !t.starts_with(';') && (
                t == format!("extension={}", name) ||
                t.starts_with(&format!("extension={} ", name)) ||
                t.starts_with(&format!("extension={};", name))
            )
        });
        PhpExtension { name: name.to_string(), enabled, category: category.to_string() }
    }).collect()
}

pub fn apply_extension_changes(content: &str, updates: &[PhpExtension]) -> String {
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    for ext in updates {
        let ext_line  = format!("extension={}", ext.name);
        let commented = format!(";extension={}", ext.name);
        let pos = lines.iter().position(|l| {
            let t = l.trim();
            t == ext_line || (t.starts_with("extension=") && t.contains(&ext.name)) ||
            t == commented || t.starts_with(&format!(";extension={}", ext.name))
        });
        if let Some(idx) = pos {
            lines[idx] = if ext.enabled { ext_line } else { format!(";{}", ext_line.clone()) };
        } else if ext.enabled {
            lines.push(ext_line);
        }
    }
    let mut result = lines.join("\n");
    if content.ends_with('\n') { result.push('\n'); }
    result
}

pub fn validate_extension_name(name: &str) -> Result<(), String> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("Extension name '{}' contains invalid characters", name));
    }
    Ok(())
}

// ── Settings ──────────────────────────────────────────────────────────────────

const KNOWN_SETTINGS: &[(&str, &str, &str)] = &[
    ("max_input_vars",      "Max Input Vars",       "Max number of form fields (default 1000)."),
    ("post_max_size",       "Max POST Size",        "Max size of POST data, e.g. 8M, 64M."),
    ("upload_max_filesize", "Max Upload Size",      "Max size of a single uploaded file."),
    ("memory_limit",        "Memory Limit",         "PHP memory limit per request, e.g. 128M."),
    ("max_execution_time",  "Max Execution Time",   "Max time (seconds) a script can run."),
    ("max_input_time",      "Max Input Time",       "Max time (seconds) to parse request data."),
    ("error_reporting",     "Error Reporting",      "PHP error reporting level, e.g. E_ALL."),
    ("display_errors",      "Display Errors",       "Show errors in browser output: On | Off."),
    ("log_errors",          "Log Errors",           "Write errors to log file: On | Off."),
    ("date.timezone",       "Timezone",             "PHP timezone, e.g. America/Lima, UTC."),
    ("default_charset",     "Default Charset",      "Default encoding, e.g. UTF-8."),
    ("intl.default_locale", "Default Locale",       "ICU locale, e.g. es_PE, en_US."),
];

pub fn parse_settings(content: &str) -> Vec<PhpSetting> {
    KNOWN_SETTINGS.iter().map(|(key, label, hint)| {
        let value = content.lines()
            .filter(|l| !l.trim().starts_with(';'))
            .find_map(|line| {
                let t = line.trim();
                let p1 = format!("{} =", key);
                let p2 = format!("{}=", key);
                if t.starts_with(&p1)      { Some(t[p1.len()..].trim().to_string()) }
                else if t.starts_with(&p2) { Some(t[p2.len()..].trim().to_string()) }
                else                       { None }
            })
            .unwrap_or_default();
        PhpSetting { key: key.to_string(), value, label: label.to_string(), hint: hint.to_string() }
    }).collect()
}

pub fn validate_setting(key: &str, value: &str) -> Result<(), String> {
    let allowed: Vec<&str> = KNOWN_SETTINGS.iter().map(|(k, _, _)| *k).collect();
    if !allowed.contains(&key) {
        return Err(format!("Setting '{}' is not allowed", key));
    }
    if value.is_empty() { return Ok(()); }
    if value.contains('\n') || value.contains('\r') || value.contains('\0') {
        return Err(format!("Value for '{}' contains illegal characters", key));
    }
    match key {
        "max_input_vars" | "max_execution_time" | "max_input_time" => {
            if !value.chars().all(|c| c.is_ascii_digit()) {
                return Err(format!("'{}' must be a non-negative integer", key));
            }
        }
        "post_max_size" | "upload_max_filesize" | "memory_limit" => {
            let upper = value.to_uppercase();
            let digits = match upper.chars().last() {
                Some('K') | Some('M') | Some('G') => &value[..value.len()-1],
                _ => value,
            };
            if !digits.chars().all(|c| c.is_ascii_digit()) || digits.is_empty() {
                return Err(format!("'{}' must be a size like 128M, 1G", key));
            }
        }
        "display_errors" | "log_errors" => {
            match value.to_lowercase().as_str() {
                "on" | "off" | "1" | "0" | "true" | "false" => {}
                _ => return Err(format!("'{}' must be On or Off", key)),
            }
        }
        "date.timezone" => {
            if !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.')) {
                return Err(format!("'{}' contains invalid characters for a timezone", key));
            }
        }
        "error_reporting" => {
            if !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '|' | '&' | '~' | '^' | '_')) {
                return Err(format!("'{}' contains invalid characters", key));
            }
        }
        "default_charset" => {
            if !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) {
                return Err(format!("'{}' must be a valid charset name like UTF-8", key));
            }
        }
        "intl.default_locale" => {
            if !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')) {
                return Err(format!("'{}' must be a locale like es_PE", key));
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn apply_setting_changes(content: &str, updates: &[PhpSetting]) -> String {
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    for setting in updates {
        if validate_setting(&setting.key, &setting.value).is_err() { continue; }
        let new_line = format!("{} = {}", setting.key, setting.value);
        let pos = lines.iter().position(|l| {
            let t = l.trim().trim_start_matches(';').trim();
            t.starts_with(&format!("{} =", setting.key)) ||
            t.starts_with(&format!("{}=", setting.key))
        });
        if let Some(idx) = pos {
            lines[idx] = if setting.value.is_empty() { format!(";{}", new_line) } else { new_line };
        } else if !setting.value.is_empty() {
            lines.push(new_line);
        }
    }
    let mut result = lines.join("\n");
    if content.ends_with('\n') { result.push('\n'); }
    result
}
