use std::path::{Path, PathBuf};
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

/// Project types worth looking one level down for: a framework or a built SPA.
/// A bare `index.html` (Static) is too common to be a signal.
fn is_framework(project_type: &ProjectType) -> bool {
    matches!(
        project_type,
        ProjectType::Laravel
            | ProjectType::CodeIgniter4
            | ProjectType::CodeIgniter3
            | ProjectType::WordPress
            | ProjectType::Spa
    )
}

/// A folder that only wraps the real project (`laravel/` holding `todoList/`
/// with `artisan` and `public/`). When `path` itself is not a recognised
/// project (`detect` says Generic) and exactly one of its visible subfolders is
/// a framework project, returns that subfolder and its type. Zero or several
/// candidates are ambiguous and return `None`, as does an unreadable folder.
pub fn find_nested_project(path: &str) -> Option<(PathBuf, ProjectType)> {
    if detect(path) != ProjectType::Generic {
        return None;
    }
    let mut found = std::fs::read_dir(path)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|child| child.is_dir() && !is_hidden(child))
        .filter_map(|child| {
            let project_type = detect(&child.to_string_lossy());
            is_framework(&project_type).then_some((child, project_type))
        });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

fn is_hidden(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

/// Where a new site should point and what it is: `path` itself, or the single
/// framework project it wraps (see `find_nested_project`).
pub fn resolve_site_root(path: &str) -> (String, ProjectType) {
    match find_nested_project(path) {
        Some((nested, project_type)) => (nested.to_string_lossy().into_owned(), project_type),
        None => (path.to_string(), detect(path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_laravel(dir: &Path) {
        fs::create_dir_all(dir.join("public")).unwrap();
        fs::write(dir.join("artisan"), "").unwrap();
    }

    fn path_str(p: &Path) -> String {
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn detects_generic_for_empty_path() {
        assert_eq!(detect("/nonexistent/path/xyz"), ProjectType::Generic);
    }

    #[test]
    fn nested_finds_the_single_laravel_subfolder() {
        let tmp = TempDir::new().unwrap();
        let app = tmp.path().join("todoList");
        make_laravel(&app);
        fs::create_dir(tmp.path().join("notes")).unwrap();

        assert_eq!(
            find_nested_project(&path_str(tmp.path())),
            Some((app, ProjectType::Laravel))
        );
    }

    #[test]
    fn nested_finds_wordpress_spa_and_codeigniter() {
        let cases: [(&str, &[&str], ProjectType); 4] = [
            ("wp", &["wp-login.php"], ProjectType::WordPress),
            ("ci4", &["spark"], ProjectType::CodeIgniter4),
            ("ci3", &["application/.keep", "system/.keep", "index.php"], ProjectType::CodeIgniter3),
            ("app", &["dist/index.html"], ProjectType::Spa),
        ];
        for (name, files, expected) in cases {
            let tmp = TempDir::new().unwrap();
            let child = tmp.path().join(name);
            for file in files {
                let f = child.join(file);
                fs::create_dir_all(f.parent().unwrap()).unwrap();
                fs::write(f, "").unwrap();
            }
            assert_eq!(find_nested_project(&path_str(tmp.path())), Some((child, expected)));
        }
    }

    #[test]
    fn nested_ignores_a_folder_that_is_already_a_project() {
        let tmp = TempDir::new().unwrap();
        make_laravel(tmp.path());
        make_laravel(&tmp.path().join("inner"));
        assert_eq!(find_nested_project(&path_str(tmp.path())), None);
    }

    #[test]
    fn nested_is_none_when_ambiguous_or_absent() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(find_nested_project(&path_str(tmp.path())), None, "empty folder");

        fs::create_dir_all(tmp.path().join("site")).unwrap();
        fs::write(tmp.path().join("site").join("index.html"), "").unwrap();
        assert_eq!(find_nested_project(&path_str(tmp.path())), None, "static is not a framework");

        make_laravel(&tmp.path().join("a"));
        make_laravel(&tmp.path().join("b"));
        assert_eq!(find_nested_project(&path_str(tmp.path())), None, "two candidates");
    }

    #[test]
    fn nested_skips_hidden_folders() {
        let tmp = TempDir::new().unwrap();
        make_laravel(&tmp.path().join(".git"));
        let app = tmp.path().join("app");
        make_laravel(&app);
        assert_eq!(find_nested_project(&path_str(tmp.path())), Some((app, ProjectType::Laravel)));
    }

    #[test]
    fn nested_is_none_for_a_missing_folder() {
        assert_eq!(find_nested_project("/nonexistent/path/xyz"), None);
    }

    #[test]
    fn resolve_site_root_falls_back_to_the_folder_itself() {
        let tmp = TempDir::new().unwrap();
        let root = path_str(tmp.path());
        assert_eq!(resolve_site_root(&root), (root.clone(), ProjectType::Generic));

        let app = tmp.path().join("todoList");
        make_laravel(&app);
        assert_eq!(resolve_site_root(&root), (path_str(&app), ProjectType::Laravel));
    }
}
