use std::path::Path;
use crate::domain::site::entity::ProjectType;

/// Infiere el tipo de proyecto a partir del contenido del directorio.
/// Extrae la lógica de `routes::detect_project_type` al dominio de aplicación.
/// Es una función pura: no tiene estado, no tiene dependencias inyectadas.
pub fn detect(path: &str) -> ProjectType {
    let base = Path::new(path);
    if base.join("artisan").exists() && base.join("public").exists() {
        return ProjectType::Laravel;
    }
    if base.join("spark").exists() {
        return ProjectType::CodeIgniter4;
    }
    if base.join("application").exists()
        && base.join("system").exists()
        && base.join("index.php").exists()
    {
        return ProjectType::CodeIgniter3;
    }
    if base.join("wp-config.php").exists() || base.join("wp-login.php").exists() {
        return ProjectType::WordPress;
    }
    if base.join("dist").join("index.html").exists()
        || base.join("build").join("index.html").exists()
    {
        return ProjectType::Spa;
    }
    if base.join("index.html").exists() {
        return ProjectType::Static;
    }
    ProjectType::Generic
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_generic_for_empty_path() {
        assert_eq!(detect("/nonexistent/path/xyz"), ProjectType::Generic);
    }
}
