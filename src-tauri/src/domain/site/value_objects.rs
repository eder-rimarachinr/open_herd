use crate::domain::errors::DomainError;

// ── SiteId ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SiteId(String);

impl Default for SiteId {
    fn default() -> Self {
        Self::new()
    }
}

impl SiteId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SiteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── DomainName ────────────────────────────────────────────────────────────────

/// Nombre de dominio válido para open-herd. Siempre termina en `.test`.
/// Inmutable — solo puede construirse a través de `DomainName::new`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DomainName(String);

impl DomainName {
    /// Valida y construye un `DomainName`. Absorbe la lógica de `validate::domain`.
    pub fn new(raw: &str) -> Result<Self, DomainError> {
        if raw.is_empty() {
            return Err(DomainError::InvalidDomain("el dominio no puede estar vacío".into()));
        }
        if !raw.ends_with(".test") {
            return Err(DomainError::InvalidTld);
        }
        if !raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
            return Err(DomainError::InvalidDomain(
                "solo se permiten letras ASCII, dígitos, guiones y puntos".into(),
            ));
        }
        if raw.split('.').any(str::is_empty) {
            return Err(DomainError::InvalidDomain("etiqueta vacía en el dominio".into()));
        }
        Ok(Self(raw.to_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DomainName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── PhpVersion ────────────────────────────────────────────────────────────────

/// Versión de PHP representada como par (major, minor).
/// Encapsula el cálculo del puerto FastCGI para que no viva en site_config.rs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhpVersion {
    pub major: u8,
    pub minor: u8,
}

impl PhpVersion {
    pub fn new(major: u8, minor: u8) -> Self {
        Self { major, minor }
    }

    /// Parsea una cadena "8.2" o "8.2.1". Devuelve `None` si el formato no es válido.
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.split('.').filter_map(|p| p.parse::<u8>().ok());
        let major = parts.next()?;
        let minor = parts.next().unwrap_or(0);
        Some(Self { major, minor })
    }

    /// Puerto FastCGI: 9000 + major*10 + minor  (ej. 8.2 → 9082)
    /// Lógica extraída de `site_config::fastcgi_port`.
    pub fn fastcgi_port(&self) -> u16 {
        9000 + (self.major as u16 * 10) + self.minor as u16
    }

    pub fn display(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }
}

impl std::fmt::Display for PhpVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

// ── SitePath ─────────────────────────────────────────────────────────────────

/// Ruta absoluta en el sistema de archivos validada en construcción.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePath(std::path::PathBuf);

impl SitePath {
    pub fn new(raw: &str) -> Result<Self, DomainError> {
        if raw.is_empty() {
            return Err(DomainError::InvalidPath("la ruta no puede estar vacía".into()));
        }
        if raw.chars().any(char::is_control) {
            return Err(DomainError::InvalidPath("la ruta contiene caracteres de control".into()));
        }
        let path = std::path::Path::new(raw);
        if !path.is_absolute() {
            return Err(DomainError::InvalidPath("la ruta debe ser absoluta".into()));
        }
        Ok(Self(path.to_path_buf()))
    }

    pub fn as_path(&self) -> &std::path::Path {
        &self.0
    }
}

impl std::fmt::Display for SitePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_name_valid() {
        assert!(DomainName::new("myapp.test").is_ok());
        assert!(DomainName::new("my-app.test").is_ok());
        assert!(DomainName::new("sub.myapp.test").is_ok());
    }

    #[test]
    fn domain_name_wrong_tld() {
        assert_eq!(DomainName::new("myapp.com"), Err(DomainError::InvalidTld));
    }

    #[test]
    fn domain_name_empty_label() {
        assert!(DomainName::new("my..test").is_err());
    }

    #[test]
    fn domain_name_injection() {
        assert!(DomainName::new("evil.test\nreturn 200;").is_err());
        assert!(DomainName::new("evil.test; rm -rf /").is_err());
    }

    #[test]
    fn php_version_fastcgi_port() {
        assert_eq!(PhpVersion::new(8, 2).fastcgi_port(), 9082);
        assert_eq!(PhpVersion::new(7, 4).fastcgi_port(), 9074);
        assert_eq!(PhpVersion::new(8, 1).fastcgi_port(), 9081);
    }

    #[test]
    fn php_version_parse() {
        let v = PhpVersion::parse("8.2.1").unwrap();
        assert_eq!(v.major, 8);
        assert_eq!(v.minor, 2);

        let v = PhpVersion::parse("8.2").unwrap();
        assert_eq!(v.major, 8);
        assert_eq!(v.minor, 2);
    }

    #[test]
    fn site_path_must_be_absolute() {
        assert!(SitePath::new("relative/path").is_err());
    }

    #[test]
    fn site_path_control_chars() {
        assert!(SitePath::new("/home/user/proj\nect").is_err());
    }
}
