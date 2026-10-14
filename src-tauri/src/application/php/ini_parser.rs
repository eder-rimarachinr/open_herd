//! Reading and editing php.ini text. Pure functions: no I/O, no infrastructure
//! types (the HTTP DTOs are mapped from these in `infrastructure::dto`).

// ── Model ─────────────────────────────────────────────────────────────────────

/// A known extension and whether php.ini currently loads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionState {
    pub name: String,
    pub enabled: bool,
    pub category: String,
}

/// A known setting and its current value (empty when unset or commented out).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingValue {
    pub key: String,
    pub value: String,
    pub label: String,
    pub hint: String,
}

/// Request to load (`enabled`) or stop loading an extension.
#[derive(Debug, Clone)]
pub struct ExtensionChange {
    pub name: String,
    pub enabled: bool,
}

/// Request to set a setting; an empty `value` comments the line out.
#[derive(Debug, Clone)]
pub struct SettingChange {
    pub key: String,
    pub value: String,
}

// ── Extensions ────────────────────────────────────────────────────────────────

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

/// If `line` is an `extension=` directive (active or commented out), returns
/// the extension's bare name and whether the line is active. Accepts the
/// spellings found in real php.ini files: `extension=curl`,
/// `extension="curl"`, `extension = curl ; comment`, `;extension=curl` and the
/// legacy `extension=php_curl.dll` / `curl.so`.
fn extension_directive(line: &str) -> Option<(&str, bool)> {
    let trimmed = line.trim();
    let (body, enabled) = match trimmed.strip_prefix(';') {
        Some(rest) => (rest.trim_start(), false),
        None => (trimmed, true),
    };
    let value = body.strip_prefix("extension")?.trim_start().strip_prefix('=')?;
    let value = value.split(';').next()?.trim().trim_matches('"').trim_matches('\'');
    let name = value.strip_prefix("php_").unwrap_or(value);
    let name = name.strip_suffix(".dll").or_else(|| name.strip_suffix(".so")).unwrap_or(name);
    (!name.is_empty()).then_some((name, enabled))
}

pub fn parse_extensions(content: &str) -> Vec<ExtensionState> {
    KNOWN_EXTENSIONS.iter().map(|(name, category)| {
        let enabled = content.lines()
            .filter_map(extension_directive)
            .any(|(n, active)| active && n == *name);
        ExtensionState { name: (*name).to_string(), enabled, category: (*category).to_string() }
    }).collect()
}

pub fn apply_extension_changes(content: &str, changes: &[ExtensionChange]) -> String {
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    for change in changes {
        let directive = format!("extension={}", change.name);
        let wanted = if change.enabled { directive.clone() } else { format!(";{directive}") };
        // Prefer an active line for this exact extension, then a commented one.
        let matching = |active: bool| lines.iter().position(|l| {
            extension_directive(l).is_some_and(|(n, a)| n == change.name && a == active)
        });
        match matching(true).or_else(|| matching(false)) {
            Some(idx) => lines[idx] = wanted,
            None if change.enabled => lines.push(directive),
            None => {}
        }
    }
    join_like(content, &lines)
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

/// `key = value` or `key=value` (no leading `;`): returns the raw value text.
fn setting_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.trim().strip_prefix(key)?.trim_start().strip_prefix('=')
}

pub fn parse_settings(content: &str) -> Vec<SettingValue> {
    KNOWN_SETTINGS.iter().map(|(key, label, hint)| {
        let value = content.lines()
            .filter(|l| !l.trim().starts_with(';'))
            .find_map(|l| setting_value(l, key))
            // php.ini may quote values: default_charset = "UTF-8"
            .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
            .unwrap_or_default();
        SettingValue { key: (*key).to_string(), value, label: (*label).to_string(), hint: (*hint).to_string() }
    }).collect()
}

pub fn validate_setting(key: &str, value: &str) -> Result<(), String> {
    if !KNOWN_SETTINGS.iter().any(|(k, _, _)| *k == key) {
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
            let digits = value.strip_suffix(['K', 'M', 'G', 'k', 'm', 'g']).unwrap_or(value);
            if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
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
        "intl.default_locale"
            if !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')) => {
                return Err(format!("'{}' must be a locale like es_PE", key));
            }
        _ => {}
    }
    Ok(())
}

/// Callers must validate first (`validate_setting`); invalid changes are skipped.
pub fn apply_setting_changes(content: &str, changes: &[SettingChange]) -> String {
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    for change in changes {
        if validate_setting(&change.key, &change.value).is_err() { continue; }
        let new_line = format!("{} = {}", change.key, change.value);
        let pos = lines.iter().position(|l| setting_value(l.trim().trim_start_matches(';'), &change.key).is_some());
        match pos {
            Some(idx) => lines[idx] = if change.value.is_empty() { format!(";{new_line}") } else { new_line },
            None if !change.value.is_empty() => lines.push(new_line),
            None => {}
        }
    }
    join_like(content, &lines)
}

/// Re-joins edited lines keeping the original line ending (php.ini files from
/// windows.php.net are CRLF) and trailing newline.
fn join_like(original: &str, lines: &[String]) -> String {
    let eol = if original.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = lines.join(eol);
    if original.ends_with('\n') { out.push_str(eol); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toggle(name: &str, enabled: bool) -> ExtensionChange {
        ExtensionChange { name: name.into(), enabled }
    }
    fn set(key: &str, value: &str) -> SettingChange {
        SettingChange { key: key.into(), value: value.into() }
    }
    fn enabled(ini: &str, name: &str) -> bool {
        parse_extensions(ini).into_iter().any(|e| e.name == name && e.enabled)
    }

    #[test]
    fn disabling_xml_does_not_touch_xmlrpc() {
        // Used to rewrite the xmlrpc line into ";extension=xml" (substring match).
        let ini = "extension=xmlrpc\nextension=xml\n";
        assert_eq!(apply_extension_changes(ini, &[toggle("xml", false)]), "extension=xmlrpc\n;extension=xml\n");
    }

    #[test]
    fn enabling_uncomments_the_exact_extension() {
        let ini = ";extension=xsl\n;extension=xml\n";
        assert_eq!(apply_extension_changes(ini, &[toggle("xml", true)]), ";extension=xsl\nextension=xml\n");
    }

    #[test]
    fn enabling_a_missing_extension_appends_it() {
        assert_eq!(apply_extension_changes("[PHP]\n", &[toggle("intl", true)]), "[PHP]\nextension=intl\n");
    }

    #[test]
    fn parses_real_world_spellings() {
        let ini = "extension=php_curl.dll\nextension = \"gd\" ; images\n;extension=zip\nextension=mbstring.so\n";
        assert!(enabled(ini, "curl"));
        assert!(enabled(ini, "gd"));
        assert!(enabled(ini, "mbstring"));
        assert!(!enabled(ini, "zip"));
        assert!(!enabled(ini, "zlib"), "a commented zip line must not count for zlib");
    }

    #[test]
    fn crlf_files_stay_crlf() {
        let ini = ";extension=intl\r\nmemory_limit = 128M\r\n";
        let out = apply_setting_changes(&apply_extension_changes(ini, &[toggle("intl", true)]), &[set("memory_limit", "256M")]);
        assert_eq!(out, "extension=intl\r\nmemory_limit = 256M\r\n");
    }

    #[test]
    fn settings_are_set_commented_out_and_appended() {
        let ini = "memory_limit = 128M\n;date.timezone =\n";
        let out = apply_setting_changes(ini, &[set("memory_limit", ""), set("date.timezone", "UTC"), set("max_input_vars", "3000")]);
        assert_eq!(out, ";memory_limit = \ndate.timezone = UTC\nmax_input_vars = 3000\n");
    }

    #[test]
    fn parses_quoted_and_unspaced_settings() {
        let s = parse_settings("default_charset = \"UTF-8\"\nmax_input_vars=5000\n;memory_limit = 1G\n");
        let get = |k: &str| s.iter().find(|x| x.key == k).map(|x| x.value.clone()).unwrap_or_default();
        assert_eq!(get("default_charset"), "UTF-8");
        assert_eq!(get("max_input_vars"), "5000");
        assert_eq!(get("memory_limit"), "", "commented lines are not the current value");
    }

    #[test]
    fn validation_rejects_injection_and_bad_values() {
        assert!(validate_setting("memory_limit", "128M\nauto_prepend_file=/evil.php").is_err());
        assert!(validate_setting("memory_limit", "lots").is_err());
        assert!(validate_setting("memory_limit", "M").is_err());
        assert!(validate_setting("disable_functions", "").is_err(), "unknown keys are refused");
        assert!(validate_setting("display_errors", "maybe").is_err());
        assert!(validate_extension_name("curl\nextension=evil").is_err());
        assert!(validate_setting("memory_limit", "512m").is_ok());
        assert!(validate_setting("date.timezone", "America/Lima").is_ok());
        assert!(validate_extension_name("pdo_mysql").is_ok());
    }
}
