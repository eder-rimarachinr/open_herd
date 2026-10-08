use super::value_objects::{DomainName, PhpVersion, SiteId, SitePath};
use crate::domain::errors::DomainError;

// ── ProjectType ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectType {
    Laravel,
    CodeIgniter4,
    CodeIgniter3,
    WordPress,
    Spa,
    Static,
    Generic,
}

impl ProjectType {
    /// Carpeta pública relativa a la raíz del sitio.
    /// Absorbe la lógica de `site_config::document_root`.
    pub fn document_root(&self) -> &str {
        match self {
            Self::Laravel | Self::CodeIgniter4 => "public",
            // SPA: el adaptador de nginx elige dist/ o build/ en tiempo de ejecución
            Self::Spa => "dist",
            _ => "",
        }
    }

    /// Indica si nginx debe habilitar autoindex para este tipo de proyecto.
    pub fn autoindex(&self) -> bool {
        matches!(self, Self::Generic)
    }
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Laravel => "laravel",
            Self::CodeIgniter4 => "codeigniter4",
            Self::CodeIgniter3 => "codeigniter3",
            Self::WordPress => "wordpress",
            Self::Spa => "spa",
            Self::Static => "static",
            Self::Generic => "generic",
        };
        f.write_str(s)
    }
}

impl std::str::FromStr for ProjectType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        Ok(match s {
            "laravel" => Self::Laravel,
            "codeigniter4" => Self::CodeIgniter4,
            "codeigniter3" => Self::CodeIgniter3,
            "wordpress" => Self::WordPress,
            "spa" => Self::Spa,
            "static" => Self::Static,
            _ => Self::Generic,
        })
    }
}

// ── SslStatus ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SslStatus {
    Disabled,
    Active {
        cert_path: String,
        key_path: String,
    },
}

impl SslStatus {
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Active { .. })
    }
}

// ── Site (Aggregate Root) ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Site {
    pub id: SiteId,
    pub name: String,
    pub domain: DomainName,
    pub path: SitePath,
    pub php_version: Option<PhpVersion>,
    pub ssl: SslStatus,
    pub project_type: ProjectType,
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Site {
    pub fn new(name: String, domain: DomainName, path: SitePath) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: SiteId::new(),
            name,
            domain,
            path,
            php_version: None,
            ssl: SslStatus::Disabled,
            project_type: ProjectType::Generic,
            active: true,
            created_at: now,
            updated_at: now,
        }
    }

    // ── Reglas de negocio ─────────────────────────────────────────────────────

    /// SSL solo puede activarse si aún no está activo.
    pub fn can_enable_ssl(&self) -> bool {
        !self.ssl.is_enabled()
    }

    /// Activa SSL almacenando las rutas de los certificados emitidos.
    pub fn enable_ssl(&mut self, cert_path: String, key_path: String) -> Result<(), DomainError> {
        if !self.can_enable_ssl() {
            return Err(DomainError::SslAlreadyActive(self.domain.to_string()));
        }
        self.ssl = SslStatus::Active { cert_path, key_path };
        self.touch();
        Ok(())
    }

    /// Desactiva SSL. Es idempotente.
    pub fn disable_ssl(&mut self) {
        self.ssl = SslStatus::Disabled;
        self.touch();
    }

    /// Cambia la versión de PHP asignada al sitio.
    pub fn set_php_version(&mut self, version: PhpVersion) {
        self.php_version = Some(version);
        self.touch();
    }

    /// Cambia el tipo de proyecto detectado.
    pub fn set_project_type(&mut self, project_type: ProjectType) {
        self.project_type = project_type;
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = chrono::Utc::now();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_site() -> Site {
        let domain = DomainName::new("myapp.test").unwrap();
        #[cfg(target_os = "windows")]
        let path = SitePath::new(r"C:\Users\dev\myapp").unwrap();
        #[cfg(not(target_os = "windows"))]
        let path = SitePath::new("/home/dev/myapp").unwrap();
        Site::new("myapp".into(), domain, path)
    }

    #[test]
    fn ssl_enable_disable_roundtrip() {
        let mut site = make_site();
        assert!(site.can_enable_ssl());

        site.enable_ssl("cert.pem".into(), "key.pem".into()).unwrap();
        assert!(site.ssl.is_enabled());
        assert!(!site.can_enable_ssl());

        site.disable_ssl();
        assert!(!site.ssl.is_enabled());
        assert!(site.can_enable_ssl());
    }

    #[test]
    fn ssl_enable_twice_is_error() {
        let mut site = make_site();
        site.enable_ssl("cert.pem".into(), "key.pem".into()).unwrap();
        let result = site.enable_ssl("cert2.pem".into(), "key2.pem".into());
        assert!(matches!(result, Err(DomainError::SslAlreadyActive(_))));
    }

    #[test]
    fn project_type_document_root() {
        assert_eq!(ProjectType::Laravel.document_root(), "public");
        assert_eq!(ProjectType::Generic.document_root(), "");
        assert!(ProjectType::Generic.autoindex());
        assert!(!ProjectType::Laravel.autoindex());
    }
}
